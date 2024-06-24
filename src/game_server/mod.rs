use std::collections::BTreeMap;
use std::io::{Cursor, Error};
use std::path::Path;
use std::vec;

use byteorder::{LittleEndian, ReadBytesExt};
use rand::Rng;

use packet_serialize::{
    DeserializePacket, DeserializePacketError, NullTerminatedString, SerializePacketError,
};
use unique_guid::{shorten_zone_template_guid, zone_instance_guid};
use zone::CharacterCategory;

use crate::game_server::chat::process_chat_packet;
use crate::game_server::client_update_packet::{
    Health, Power, PreloadCharactersDone, Stat, StatId, Stats,
};
use crate::game_server::command::process_command;
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::guid::{
    Guid, GuidTable, GuidTableHandle, GuidTableReadHandle, GuidTableWriteHandle,
};
use crate::game_server::housing::{
    process_housing_packet, HouseDescription, HouseInstanceEntry, HouseInstanceList,
};
use crate::game_server::item::make_item_definitions;
use crate::game_server::login::{
    send_points_of_interest, DeploymentEnv, GameSettings, LoginReply, WelcomeScreen,
    ZoneDetailsDone,
};
use crate::game_server::mount::{load_mounts, process_mount_packet, MountConfig};
use crate::game_server::player_data::{
    make_test_nameplate_image, make_test_player, make_test_wield_type,
};
use crate::game_server::player_update_packet::make_test_npc;
use crate::game_server::reference_data::{
    CategoryDefinition, CategoryDefinitions, CategoryRelation, ItemGroupDefinitions,
    ItemGroupDefinitionsData,
};
use crate::game_server::time::make_game_time_sync;
use crate::game_server::tunnel::{TunneledPacket, TunneledWorldPacket};
use crate::game_server::unique_guid::player_guid;
use crate::game_server::update_position::UpdatePlayerPosition;
use crate::game_server::zone::{
    load_zones, teleport_within_zone, Character, Zone, ZoneTeleportRequest, ZoneTemplate,
};
use crate::teleport_to_zone;

mod chat;
mod client_update_packet;
mod combat_update_packet;
mod command;
mod game_packet;
mod guid;
mod housing;
mod item;
mod login;
mod mount;
mod player_data;
mod player_update_packet;
mod purchase;
mod reference_data;
mod store;
mod time;
mod tunnel;
mod ui;
mod unique_guid;
mod update_position;
mod zone;

#[derive(Debug)]
pub enum Broadcast {
    Single(u32, Vec<Vec<u8>>),
    Multi(Vec<u32>, Vec<Vec<u8>>),
}

#[non_exhaustive]
#[derive(Debug)]
pub enum ProcessPacketError {
    CorruptedPacket,
    SerializeError(SerializePacketError),
}

impl From<Error> for ProcessPacketError {
    fn from(_: Error) -> Self {
        ProcessPacketError::CorruptedPacket
    }
}

impl From<DeserializePacketError> for ProcessPacketError {
    fn from(_: DeserializePacketError) -> Self {
        ProcessPacketError::CorruptedPacket
    }
}

impl From<SerializePacketError> for ProcessPacketError {
    fn from(value: SerializePacketError) -> Self {
        ProcessPacketError::SerializeError(value)
    }
}

pub struct GameServer {
    characters: GuidTable<u64, Character, (u64, CharacterCategory)>,
    mounts: BTreeMap<u32, MountConfig>,
    zone_templates: BTreeMap<u8, ZoneTemplate>,
    zones: GuidTable<u64, Zone>,
}

impl GameServer {
    pub fn new(config_dir: &Path) -> Result<Self, Error> {
        let characters = GuidTable::new();
        let (templates, zones) = load_zones(config_dir, characters.write())?;
        Ok(GameServer {
            characters,
            mounts: load_mounts(config_dir)?,
            zone_templates: templates,
            zones,
        })
    }

    pub fn login(&self, data: Vec<u8>) -> Result<(u32, Vec<Broadcast>), ProcessPacketError> {
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::LoginRequest => {
                    // TODO: validate and get GUID from login request
                    let guid = 1;

                    // TODO: get player's zone
                    let player_zone = 24;

                    let mut packets = Vec::new();

                    let login_reply = TunneledPacket {
                        unknown1: true,
                        inner: LoginReply { logged_in: true },
                    };
                    packets.push(GamePacket::serialize(&login_reply)?);

                    let deployment_env = TunneledPacket {
                        unknown1: true,
                        inner: DeploymentEnv {
                            environment: NullTerminatedString("prod".to_string()),
                        },
                    };
                    packets.push(GamePacket::serialize(&deployment_env)?);
                    let mut zone_details = self
                        .zones
                        .read()
                        .get(player_zone)
                        .unwrap()
                        .read()
                        .send_self()?;
                    packets.append(&mut zone_details);

                    let settings = TunneledPacket {
                        unknown1: true,
                        inner: GameSettings {
                            unknown1: 4,
                            unknown2: 7,
                            unknown3: 268,
                            unknown4: true,
                            time_scale: 1.0,
                        },
                    };
                    packets.push(GamePacket::serialize(&settings)?);

                    let item_defs = TunneledPacket {
                        unknown1: true,
                        inner: make_item_definitions(),
                    };
                    packets.push(GamePacket::serialize(&item_defs)?);

                    let player = TunneledPacket {
                        unknown1: true,
                        inner: make_test_player(guid, self.mounts()),
                    };
                    packets.push(GamePacket::serialize(&player)?);

                    let mut characters_write_handle = self.characters.write();
                    characters_write_handle.insert(player.inner.data.to_character(player_zone));

                    Ok((guid, vec![Broadcast::Single(guid, packets)]))
                }
                _ => {
                    println!("Client tried to log in without a login request");
                    Err(ProcessPacketError::CorruptedPacket)
                }
            },
            Err(_) => {
                println!("Unknown op code at login: {}", raw_op_code);
                Err(ProcessPacketError::CorruptedPacket)
            }
        }
    }

    pub fn process_packet(
        &self,
        sender: u32,
        data: Vec<u8>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let mut broadcasts = Vec::new();
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::TunneledClient => {
                    let packet: TunneledPacket<Vec<u8>> =
                        DeserializePacket::deserialize(&mut cursor)?;
                    broadcasts.append(&mut self.process_packet(sender, packet.inner)?);
                }
                OpCode::TunneledWorld => {
                    let packet: TunneledWorldPacket<Vec<u8>> =
                        DeserializePacket::deserialize(&mut cursor)?;
                    broadcasts.append(&mut self.process_packet(sender, packet.inner)?);
                }
                OpCode::ClientIsReady => {
                    let mut packets = Vec::new();

                    packets.append(&mut send_points_of_interest(self)?);

                    let categories = TunneledPacket {
                        unknown1: true,
                        inner: CategoryDefinitions {
                            definitions: vec![
                                CategoryDefinition {
                                    guid: 65,
                                    name: 1222,
                                    icon_set_id: 0,
                                    unknown1: 1,
                                    unknown2: true,
                                },
                                CategoryDefinition {
                                    guid: 66,
                                    name: 316,
                                    icon_set_id: 786,
                                    unknown1: 1,
                                    unknown2: true,
                                },
                                CategoryDefinition {
                                    guid: 67,
                                    name: 317,
                                    icon_set_id: 787,
                                    unknown1: 1,
                                    unknown2: true,
                                },
                            ],
                            relations: vec![
                                CategoryRelation {
                                    parent_guid: 65,
                                    child_guid: 66,
                                },
                                CategoryRelation {
                                    parent_guid: 65,
                                    child_guid: 67,
                                },
                            ],
                        },
                    };
                    packets.push(GamePacket::serialize(&categories)?);

                    let item_groups = TunneledPacket {
                        unknown1: true,
                        inner: ItemGroupDefinitions {
                            data: ItemGroupDefinitionsData {
                                definitions: vec![],
                            },
                        },
                    };
                    packets.push(GamePacket::serialize(&item_groups)?);

                    let npc = TunneledPacket {
                        unknown1: true,
                        inner: make_test_npc(),
                    };
                    //packets.push(GamePacket::serialize(&npc)?);

                    let characters = self.characters.read();
                    if let Some(character) = characters.get(player_guid(sender)) {
                        let character_read_handle = character.read();
                        if let Some(zone) =
                            self.read_zones().get(character_read_handle.instance_guid)
                        {
                            let zone_read_handle = zone.read();
                            let mut preloaded_npcs =
                                zone_read_handle.send_characters(&characters)?;
                            packets.append(&mut preloaded_npcs);

                            let stats = TunneledPacket {
                                unknown1: true,
                                inner: Stats {
                                    stats: vec![
                                        Stat {
                                            id: StatId::Speed,
                                            multiplier: 1,
                                            value1: 0.0,
                                            value2: zone_read_handle.speed,
                                        },
                                        Stat {
                                            id: StatId::PowerRegen,
                                            multiplier: 1,
                                            value1: 0.0,
                                            value2: 1.0,
                                        },
                                        Stat {
                                            id: StatId::PowerRegen,
                                            multiplier: 1,
                                            value1: 0.0,
                                            value2: 1.0,
                                        },
                                        Stat {
                                            id: StatId::GravityMultiplier,
                                            multiplier: 1,
                                            value1: 0.0,
                                            value2: zone_read_handle.gravity_multiplier,
                                        },
                                        Stat {
                                            id: StatId::JumpHeightMultiplier,
                                            multiplier: 1,
                                            value1: 0.0,
                                            value2: zone_read_handle.jump_height_multiplier,
                                        },
                                    ],
                                },
                            };
                            packets.push(GamePacket::serialize(&stats)?);
                        } else {
                            println!(
                                "Player {} sent a ready packet from unknown zone {}",
                                sender, character_read_handle.instance_guid
                            );
                            return Err(ProcessPacketError::CorruptedPacket);
                        }
                    } else {
                        println!(
                            "Player {} sent a ready packet but is not in any zone",
                            sender
                        );
                        return Err(ProcessPacketError::CorruptedPacket);
                    }

                    let health = TunneledPacket {
                        unknown1: true,
                        inner: Health {
                            current: 25000,
                            max: 25000,
                        },
                    };
                    packets.push(GamePacket::serialize(&health)?);

                    let power = TunneledPacket {
                        unknown1: true,
                        inner: Power {
                            current: 300,
                            max: 300,
                        },
                    };
                    packets.push(GamePacket::serialize(&power)?);

                    packets.append(&mut make_test_wield_type(sender)?);

                    packets.append(&mut make_test_nameplate_image(sender)?);

                    let welcome_screen = TunneledPacket {
                        unknown1: true,
                        inner: WelcomeScreen {
                            show_ui: true,
                            unknown1: vec![],
                            unknown2: vec![],
                            unknown3: 0,
                            unknown4: 0,
                        },
                    };
                    packets.push(GamePacket::serialize(&welcome_screen)?);

                    let zone_details_done = TunneledPacket {
                        unknown1: true,
                        inner: ZoneDetailsDone {},
                    };
                    packets.push(GamePacket::serialize(&zone_details_done)?);

                    let preload_characters_done = TunneledPacket {
                        unknown1: true,
                        inner: PreloadCharactersDone { unknown1: false },
                    };
                    packets.push(GamePacket::serialize(&preload_characters_done)?);

                    broadcasts.push(Broadcast::Single(sender, packets));
                }
                OpCode::GameTimeSync => {
                    let game_time_sync = TunneledPacket {
                        unknown1: true,
                        inner: make_game_time_sync(),
                    };
                    broadcasts.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&game_time_sync)?],
                    ));
                }
                OpCode::Command => {
                    broadcasts.append(&mut process_command(self, &mut cursor)?);
                }
                OpCode::UpdatePlayerPosition => {
                    let pos_update: UpdatePlayerPosition =
                        DeserializePacket::deserialize(&mut cursor)?;
                    let characters = self.characters.read();
                    // TODO: broadcast pos update to all players
                    broadcasts.append(&mut Zone::move_character(characters, pos_update, self)?);
                }
                OpCode::ZoneTeleportRequest => {
                    let teleport_request: ZoneTeleportRequest =
                        DeserializePacket::deserialize(&mut cursor)?;

                    let zones = self.read_zones();
                    let mut characters = self.characters.write();
                    broadcasts.append(&mut teleport_to_zone!(
                        &zones,
                        characters,
                        sender,
                        GameServer::any_instance(
                            &zones,
                            shorten_zone_template_guid(teleport_request.destination_guid)?
                        )?,
                        None,
                        None,
                        self.mounts()
                    )?);
                }
                OpCode::TeleportToSafety => {
                    let characters = self.characters.read();
                    if let Some(character) = characters.get(player_guid(sender)) {
                        let character_read_handle = character.read();
                        let zones = self.read_zones();
                        if let Some(zone) = zones.get(character_read_handle.instance_guid) {
                            let zone_read_handle = zone.read();
                            let spawn_pos = zone_read_handle.default_spawn_pos;
                            let spawn_rot = zone_read_handle.default_spawn_rot;

                            broadcasts
                                .append(&mut teleport_within_zone(sender, spawn_pos, spawn_rot)?);
                        } else {
                            println!("Player {} outside zone tried to teleport to safety", sender);
                            return Err(ProcessPacketError::CorruptedPacket);
                        }
                    } else {
                        println!("Unknown player {} tried to teleport to safety", sender);
                        return Err(ProcessPacketError::CorruptedPacket);
                    }
                }
                OpCode::Mount => {
                    broadcasts.append(&mut process_mount_packet(&mut cursor, sender, self)?);
                }
                OpCode::Housing => {
                    broadcasts.append(&mut process_housing_packet(sender, self, &mut cursor)?);
                    broadcasts.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: HouseInstanceList {
                                instances: vec![HouseInstanceEntry {
                                    description: HouseDescription {
                                        owner_guid: player_guid(sender),
                                        house_guid: zone_instance_guid(0, 100),
                                        house_name: 1987,
                                        player_given_name: "Blaster's Mustafar Lot".to_string(),
                                        owner_name: "BLASTER NICESHOT".to_string(),
                                        icon_id: 4209,
                                        unknown5: true,
                                        fixture_count: 1,
                                        unknown7: 0,
                                        furniture_score: 3,
                                        is_locked: false,
                                        unknown10: "".to_string(),
                                        unknown11: "".to_string(),
                                        rating: 4.5,
                                        total_votes: 5,
                                        is_published: false,
                                        is_rateable: false,
                                        unknown16: 0,
                                        unknown17: 0,
                                    },
                                    unknown1: player_guid(sender),
                                }],
                            },
                        })?],
                    ));
                }
                OpCode::Chat => {
                    broadcasts.append(&mut process_chat_packet(&mut cursor, sender)?);
                }
                _ => println!("Unimplemented: {:?}, {:x?}", op_code, data),
            },
            Err(_) => println!("Unknown op code: {}, {:x?}", raw_op_code, data),
        }

        Ok(broadcasts)
    }

    pub fn read_zone_templates(&self) -> &BTreeMap<u8, ZoneTemplate> {
        &self.zone_templates
    }

    pub fn mounts(&self) -> &BTreeMap<u32, MountConfig> {
        &self.mounts
    }

    pub fn read_zones(&self) -> GuidTableReadHandle<u64, Zone> {
        self.zones.read()
    }

    pub fn write_zones(&self) -> GuidTableWriteHandle<u64, Zone> {
        self.zones.write()
    }

    pub fn any_instance(
        zones: &GuidTableReadHandle<u64, Zone>,
        template_guid: u8,
    ) -> Result<u64, ProcessPacketError> {
        let instances = GameServer::zones_by_template(zones, template_guid);
        if !instances.is_empty() {
            let index = rand::thread_rng().gen_range(0..instances.len());
            Ok(instances[index])
        } else {
            Err(ProcessPacketError::CorruptedPacket)
        }
    }

    pub fn zones_by_template(
        zones: &GuidTableReadHandle<u64, Zone>,
        template_guid: u8,
    ) -> Vec<u64> {
        let mut zone_guids = Vec::new();

        for zone in zones.values() {
            let read_handle = zone.read();
            if read_handle.template_guid == template_guid {
                zone_guids.push(read_handle.guid());
            }
        }

        zone_guids
    }
}

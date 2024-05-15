use std::collections::BTreeMap;
use std::io::{Cursor, Error};
use std::path::Path;
use std::vec;

use byteorder::{LittleEndian, ReadBytesExt};
use rand::Rng;

use packet_serialize::{DeserializePacket, DeserializePacketError, NullTerminatedString, SerializePacketError};

use crate::game_server::character_guid::player_guid;
use crate::game_server::chat::process_chat_packet;
use crate::game_server::client_update_packet::{Health, Power, PreloadCharactersDone, Stat, StatId, Stats};
use crate::game_server::command::process_command;
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::guid::{Guid, GuidTable, GuidTableHandle, GuidTableReadHandle, GuidTableWriteHandle};
use crate::game_server::housing::{HouseDescription, HouseInstanceEntry, HouseInstanceList, process_housing_packet};
use crate::game_server::item::make_item_definitions;
use crate::game_server::login::{DeploymentEnv, GameSettings, LoginReply, send_points_of_interest, WelcomeScreen, ZoneDetailsDone};
use crate::game_server::mount::{load_mounts, MountConfig, process_mount_packet};
use crate::game_server::player_data::{make_test_player, make_test_wield_type};
use crate::game_server::player_update_packet::make_test_npc;
use crate::game_server::reference_data::{CategoryDefinition, CategoryDefinitions, CategoryRelation};
use crate::game_server::time::make_game_time_sync;
use crate::game_server::tunnel::{TunneledPacket, TunneledWorldPacket};
use crate::game_server::update_position::UpdatePlayerPosition;
use crate::game_server::zone::{Character, instance_guid, load_zones, teleport_within_zone, Zone, ZoneTeleportRequest, ZoneTemplate};
use crate::teleport_to_zone;

mod login;
mod player_data;
mod tunnel;
mod game_packet;
mod time;
mod client_update_packet;
mod player_update_packet;
mod command;
mod zone;
mod guid;
mod update_position;
mod ui;
mod combat_update_packet;
mod item;
mod store;
mod mount;
mod housing;
mod character_guid;
mod chat;
mod reference_data;

#[derive(Debug)]
pub enum Broadcast {
    Single(u32, Vec<Vec<u8>>),
    Multi(Vec<u32>, Vec<Vec<u8>>)
}

#[non_exhaustive]
#[derive(Debug)]
pub enum ProcessPacketError {
    CorruptedPacket,
    SerializeError(SerializePacketError)
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

#[macro_export]
macro_rules! zone_with_character_read {
    ($zones:expr, $guid:expr, |$zone_read_handle:ident, $characters:ident| $action:expr) => {
        {
            let mut result = Err(ProcessPacketError::CorruptedPacket);

            for zone in $zones {
                let $zone_read_handle = zone.read();
                let $characters = $zone_read_handle.read_characters();
                if $characters.get($guid).is_some() {
                    result = Ok($action);
                    break;
                }
            }

            result
        }
    };
}

#[macro_export]
macro_rules! zone_with_character_write {
    ($zones:expr, $guid:expr, |$zone_read_handle:ident, mut $characters:ident| $action:expr) => {
        {
            let mut result = Err(ProcessPacketError::CorruptedPacket);

            for zone in $zones {
                let $zone_read_handle = zone.read();
                let mut $characters = $zone_read_handle.write_characters();
                if $characters.get($guid).is_some() {
                    result = Ok($action);
                    break;
                }
            }

            result
        }
    };
}

pub struct GameServer {
    mounts: BTreeMap<u32, MountConfig>,
    zone_templates: BTreeMap<u32, ZoneTemplate>,
    zones: GuidTable<u64, Zone>,
}

impl GameServer {

    pub fn new(config_dir: &Path) -> Result<Self, Error> {
        let (templates, zones) = load_zones(config_dir)?;
        Ok(
            GameServer {
                mounts: load_mounts(config_dir)?,
                zone_templates: templates,
                zones,
            }
        )
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
                        inner: LoginReply {
                            logged_in: true,
                        },
                    };
                    packets.push(GamePacket::serialize(&login_reply)?);

                    let deployment_env = TunneledPacket {
                        unknown1: true,
                        inner: DeploymentEnv {
                            environment: NullTerminatedString("prod".to_string()),
                        },
                    };
                    packets.push(GamePacket::serialize(&deployment_env)?);
                    let mut zone_details = self.zones.read().get(player_zone).unwrap().read().send_self()?;
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
                        inner: make_item_definitions()
                    };
                    packets.push(GamePacket::serialize(&item_defs)?);

                    let player = TunneledPacket {
                        unknown1: true,
                        inner: make_test_player(guid, &self.mounts())
                    };
                    packets.push(GamePacket::serialize(&player)?);

                    if let Some(zone) = self.zones.read().get(player_zone) {
                        zone.read().write_characters().insert(
                            Character::from(player.inner.data)
                        );
                    } else {
                        return Err(ProcessPacketError::CorruptedPacket);
                    }

                    Ok((guid, vec![Broadcast::Single(guid, packets)]))
                },
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
    
    pub fn process_packet(&self, sender: u32, data: Vec<u8>) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let mut broadcasts = Vec::new();
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::TunneledClient => {
                    let packet: TunneledPacket<Vec<u8>> = DeserializePacket::deserialize(&mut cursor)?;
                    broadcasts.append(&mut self.process_packet(sender, packet.inner)?);
                },
                OpCode::TunneledWorld => {
                    let packet: TunneledWorldPacket<Vec<u8>> = DeserializePacket::deserialize(&mut cursor)?;
                    broadcasts.append(&mut self.process_packet(sender, packet.inner)?);
                },
                OpCode::ClientIsReady => {
                    let mut packets = Vec::new();

                    packets.append(&mut send_points_of_interest(self)?);

                    let categories = TunneledPacket {
                        unknown1: true,
                        inner: CategoryDefinitions {
                            definitions: vec![
                                CategoryDefinition {
                                    guid: 65,
                                    name: 316,
                                    icon_id: 327,
                                    unknown1: 1,
                                    unknown2: true,
                                },
                                CategoryDefinition {
                                    guid: 100,
                                    name: 316,
                                    icon_id: 327,
                                    unknown1: 1,
                                    unknown2: true,
                                }
                            ],
                            relations: vec![
                                CategoryRelation {
                                    parent_guid: 65,
                                    child_guid: 100,
                                }
                            ],
                        },
                    };
                    packets.push(GamePacket::serialize(&categories)?);

                    let npc = TunneledPacket {
                        unknown1: true,
                        inner: make_test_npc()
                    };
                    //packets.push(GamePacket::serialize(&npc)?);

                    let zones = self.read_zones();
                    zone_with_character_read!(zones.values(), player_guid(sender), |zone_read_handle, characters| {
                        let mut preloaded_npcs = zone_read_handle.send_characters()?;
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
                    })?;

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
                        inner: PreloadCharactersDone {
                            unknown1: false
                        },
                    };
                    packets.push(GamePacket::serialize(&preload_characters_done)?);

                    broadcasts.push(Broadcast::Single(sender, packets));
                },
                OpCode::GameTimeSync => {
                    let game_time_sync = TunneledPacket {
                        unknown1: true,
                        inner: make_game_time_sync(),
                    };
                    broadcasts.push(Broadcast::Single(sender, vec![GamePacket::serialize(&game_time_sync)?]));
                },
                OpCode::Command => {
                    broadcasts.append(&mut process_command(self, &mut cursor)?);
                },
                OpCode::UpdatePlayerPosition => {
                    let pos_update: UpdatePlayerPosition = DeserializePacket::deserialize(&mut cursor)?;
                    let zones = self.read_zones();
                    zone_with_character_read!(zones.values(), player_guid(sender), |zone, characters| {

                        // TODO: broadcast pos update to all players
                        broadcasts.append(&mut Zone::move_character(characters, pos_update, self)?);

                    })?;
                },
                OpCode::ZoneTeleportRequest => {
                    let teleport_request: ZoneTeleportRequest = DeserializePacket::deserialize(&mut cursor)?;

                    let zones = self.read_zones();
                    zone_with_character_write!(zones.values(), player_guid(sender), |zone_read_handle, mut characters| {
                        broadcasts.append(
                            &mut teleport_to_zone!(
                                &zones,
                                zone_read_handle,
                                characters,
                                sender,
                                GameServer::any_instance(&zones, teleport_request.destination_guid)?,
                                None,
                                None,
                                self.mounts()
                            )?
                        );
                    })?;
                },
                OpCode::TeleportToSafety => {
                    let zones = self.read_zones();
                    zone_with_character_read!(zones.values(), player_guid(sender), |zone, characters| {
                        let spawn_pos = zone.default_spawn_pos;
                        let spawn_rot = zone.default_spawn_rot;

                        broadcasts.append(&mut teleport_within_zone(
                            sender,
                            spawn_pos,
                            spawn_rot
                        )?);
                    })?;
                },
                OpCode::Mount => {
                    broadcasts.append(&mut process_mount_packet(&mut cursor, sender, &self)?);
                },
                OpCode::Housing => {
                    broadcasts.append(&mut process_housing_packet(sender, &self, &mut cursor)?);
                    broadcasts.push(Broadcast::Single(sender, vec![
                        GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: HouseInstanceList {
                                instances: vec![
                                    HouseInstanceEntry {
                                        description: HouseDescription {
                                            owner_guid: player_guid(sender),
                                            house_guid: instance_guid(0, 100),
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
                                        unknown1: player_guid(sender)
                                    }
                                ],
                            },
                        })?
                    ]));
                },
                OpCode::Chat => {
                    broadcasts.append(&mut process_chat_packet(&mut cursor, sender)?);
                },
                _ => println!("Unimplemented: {:?}, {:x?}", op_code, data)
            },
            Err(_) => println!("Unknown op code: {}, {:x?}", raw_op_code, data)
        }

        Ok(broadcasts)
    }

    pub fn read_zone_templates(&self) -> &BTreeMap<u32, ZoneTemplate> {
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

    pub fn any_instance(zones: &GuidTableReadHandle<u64, Zone>, template_guid: u32) -> Result<u64, ProcessPacketError> {
        let instances = GameServer::zones_by_template(zones, template_guid);
        if instances.len() > 0 {
            let index = rand::thread_rng().gen_range(0..instances.len());
            Ok(instances[index])
        } else {
            Err(ProcessPacketError::CorruptedPacket)
        }
    }

    pub fn zones_by_template(zones: &GuidTableReadHandle<u64, Zone>, template_guid: u32) -> Vec<u64> {
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

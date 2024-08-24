use std::collections::BTreeMap;
use std::io::{Cursor, Error};
use std::path::Path;
use std::vec;

use byteorder::{LittleEndian, ReadBytesExt};
use handlers::character::{BattleClass, Character, CharacterIndex, CharacterType, Player};
use handlers::chat::process_chat_packet;
use handlers::command::process_command;
use handlers::guid::{GuidTable, GuidTableHandle, GuidTableWriteHandle};
use handlers::housing::process_housing_packet;
use handlers::inventory::{
    load_default_sabers, process_inventory_packet, update_saber_tints, DefaultSaber,
};
use handlers::item::load_item_definitions;
use handlers::lock_enforcer::{
    CharacterLockRequest, LockEnforcer, LockEnforcerSource, ZoneLockRequest, ZoneTableReadHandle,
};
use handlers::login::send_points_of_interest;
use handlers::mount::{load_mounts, process_mount_packet, MountConfig};
use handlers::reference_data::{load_categories, load_item_classes};
use handlers::test_data::{make_test_nameplate_image, make_test_player};
use handlers::time::make_game_time_sync;
use handlers::unique_guid::{player_guid, shorten_zone_template_guid, zone_instance_guid};
use handlers::zone::{load_zones, teleport_within_zone, Zone, ZoneTemplate};
use packets::client_update::{Health, Power, PreloadCharactersDone, Stat, StatId, Stats};
use packets::housing::{HouseDescription, HouseInstanceEntry, HouseInstanceList};
use packets::item::ItemDefinition;
use packets::login::{DeploymentEnv, GameSettings, LoginReply, WelcomeScreen, ZoneDetailsDone};
use packets::player_update::{ItemDefinitionsReply, QueueAnimation, UpdateWieldType};
use packets::reference_data::{
    CategoryDefinitions, ItemClassDefinitions, ItemGroupDefinitions, ItemGroupDefinitionsData,
};
use packets::tunnel::{TunneledPacket, TunneledWorldPacket};
use packets::update_position::UpdatePlayerPosition;
use packets::zone::{ZoneCombatSettings, ZoneTeleportRequest};
use packets::{GamePacket, OpCode};
use rand::Rng;

use crate::teleport_to_zone;
use packet_serialize::{
    DeserializePacket, DeserializePacketError, NullTerminatedString, SerializePacketError,
};

mod handlers;
mod packets;

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
    categories: CategoryDefinitions,
    default_sabers: BTreeMap<u32, DefaultSaber>,
    lock_enforcer_source: LockEnforcerSource,
    items: BTreeMap<u32, ItemDefinition>,
    item_classes: ItemClassDefinitions,
    mounts: BTreeMap<u32, MountConfig>,
    zone_templates: BTreeMap<u8, ZoneTemplate>,
}

impl GameServer {
    pub fn new(config_dir: &Path) -> Result<Self, Error> {
        let characters = GuidTable::new();
        let (templates, zones) = load_zones(config_dir, characters.write())?;
        Ok(GameServer {
            categories: load_categories(config_dir)?,
            default_sabers: load_default_sabers(config_dir)?,
            lock_enforcer_source: LockEnforcerSource::from(characters, zones),
            items: load_item_definitions(config_dir)?,
            item_classes: load_item_classes(config_dir)?,
            mounts: load_mounts(config_dir)?,
            zone_templates: templates,
        })
    }

    pub fn login(&self, data: Vec<u8>) -> Result<(u32, Vec<Broadcast>), ProcessPacketError> {
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::LoginRequest => {
                    self.lock_enforcer().write_characters(
                        |characters_write_handle, zone_lock_enforcer| {
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

                            packets.append(&mut zone_lock_enforcer.read_zones(|_| {
                                ZoneLockRequest {
                                    read_guids: vec![player_zone],
                                    write_guids: Vec::new(),
                                    zone_consumer: |_, zones_read, _| {
                                        zones_read.get(&player_zone).unwrap().send_self()
                                    },
                                }
                            })?);

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
                                inner: ItemDefinitionsReply {
                                    definitions: &self.items,
                                },
                            };
                            packets.push(GamePacket::serialize(&item_defs)?);

                            let player = TunneledPacket {
                                unknown1: true,
                                inner: make_test_player(guid, self.mounts(), &self.items),
                            };
                            packets.push(GamePacket::serialize(&player)?);

                            characters_write_handle.insert(Character::from_player(
                                guid,
                                player.inner.data.pos,
                                player.inner.data.rot,
                                player_zone,
                                Player {
                                    battle_classes: player
                                        .inner
                                        .data
                                        .battle_classes
                                        .into_iter()
                                        .map(|(battle_class_guid, battle_class)| {
                                            (
                                                battle_class_guid,
                                                BattleClass {
                                                    items: battle_class.items,
                                                },
                                            )
                                        })
                                        .collect(),
                                    active_battle_class: player.inner.data.active_battle_class,
                                    inventory: player.inner.data.inventory.into_keys().collect(),
                                },
                                self,
                            ));

                            Ok((guid, vec![Broadcast::Single(guid, packets)]))
                        },
                    )
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
                    let mut sender_only_packets = Vec::new();

                    sender_only_packets.append(&mut send_points_of_interest(self)?);

                    let categories = TunneledPacket {
                        unknown1: true,
                        inner: GamePacket::serialize(&self.categories)?,
                    };
                    sender_only_packets.push(GamePacket::serialize(&categories)?);

                    let item_groups = TunneledPacket {
                        unknown1: true,
                        inner: ItemGroupDefinitions {
                            data: ItemGroupDefinitionsData {
                                definitions: vec![],
                            },
                        },
                    };
                    sender_only_packets.push(GamePacket::serialize(&item_groups)?);

                    let mut character_broadcasts = self.lock_enforcer().read_characters(|characters_table_read_handle| {
                        let possible_index = characters_table_read_handle.index(player_guid(sender));
                        let character_guids = possible_index.map(|(instance_guid, chunk, _)| Zone::diff_character_guids(
                            instance_guid,
                            Character::MIN_CHUNK,
                            chunk,
                            characters_table_read_handle,
                            sender
                        ))
                            .unwrap_or_default();

                        let mut read_character_guids: Vec<u64> = character_guids.keys().copied().collect();
                        read_character_guids.push(player_guid(sender));
                        CharacterLockRequest {
                            read_guids: read_character_guids,
                            write_guids: Vec::new(),
                            character_consumer: move |_, characters_read, _, zones_lock_enforcer| {
                                if let Some((instance_guid, _, _)) = possible_index {
                                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                        read_guids: vec![instance_guid],
                                        write_guids: Vec::new(),
                                        zone_consumer: |_, zones_read, _| {
                                            if let Some(zone) = zones_read.get(&instance_guid) {
                                                let mut global_packets = Vec::new();
                                                let stats = TunneledPacket {
                                                    unknown1: true,
                                                    inner: Stats {
                                                        stats: vec![
                                                            Stat {
                                                                id: StatId::Speed,
                                                                multiplier: 1,
                                                                value1: 0.0,
                                                                value2: zone.speed,
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
                                                                value2: zone.gravity_multiplier,
                                                            },
                                                            Stat {
                                                                id: StatId::JumpHeightMultiplier,
                                                                multiplier: 1,
                                                                value1: 0.0,
                                                                value2: zone.jump_height_multiplier,
                                                            },
                                                        ],
                                                    },
                                                };
                                                global_packets.push(GamePacket::serialize(&stats)?);

                                                let health = TunneledPacket {
                                                    unknown1: true,
                                                    inner: Health {
                                                        current: 25000,
                                                        max: 25000,
                                                    },
                                                };
                                                global_packets.push(GamePacket::serialize(&health)?);

                                                let power = TunneledPacket {
                                                    unknown1: true,
                                                    inner: Power {
                                                        current: 300,
                                                        max: 300,
                                                    },
                                                };
                                                global_packets.push(GamePacket::serialize(&power)?);

                                                // TODO: broadcast to all
                                                let mut character_broadcasts = Vec::new();

                                                if let Some(character_read_handle) = characters_read.get(&player_guid(sender)) {
                                                    let wield_type = TunneledPacket {
                                                        unknown1: true,
                                                        inner: UpdateWieldType {
                                                            guid: player_guid(sender),
                                                            wield_type: character_read_handle.wield_type(),
                                                        },
                                                    };
                                                    global_packets.push(GamePacket::serialize(&wield_type)?);

                                                    if let CharacterType::Player(player) = &character_read_handle.character_type {
                                                        if let Some(battle_class) = player.battle_classes.get(&player.active_battle_class) {
                                                            character_broadcasts.append(&mut update_saber_tints(sender, &battle_class.items, player.active_battle_class, self)?);
                                                        }
                                                    }
                                                } else {
                                                    println!("Unknown player {} sent a ready packet", sender);
                                                    return Err(ProcessPacketError::CorruptedPacket);
                                                }

                                                character_broadcasts.push(Broadcast::Single(sender, global_packets));

                                                let mut sender_only_packets = Zone::diff_character_packets(&character_guids, &characters_read, &self.mounts)?;
                                                sender_only_packets.push(
                                                    GamePacket::serialize(&TunneledPacket {
                                                        unknown1: true,
                                                        inner: ZoneCombatSettings {
                                                            zone_guid: zone.template_guid as u32,
                                                            force_combat_pose: zone.force_combat_pose,
                                                            combat_camera: zone.is_combat,
                                                            unknown3: false,
                                                            unknown4: false,
                                                            unknown5: 0,
                                                        },
                                                    })?,
                                                );
                                                character_broadcasts.push(Broadcast::Single(sender, sender_only_packets));

                                                Ok(character_broadcasts)
                                            } else {
                                                println!(
                                                    "Player {} sent a ready packet from unknown zone {}",
                                                    sender, instance_guid
                                                );
                                                Err(ProcessPacketError::CorruptedPacket)
                                            }
                                        },
                                    })
                                } else {
                                    println!(
                                        "Player {} sent a ready packet but is not in any zone",
                                        sender
                                    );
                                    Err(ProcessPacketError::CorruptedPacket)
                                }
                            },
                        }
                    })?;
                    broadcasts.append(&mut character_broadcasts);

                    sender_only_packets.append(&mut make_test_nameplate_image(sender)?);

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
                    sender_only_packets.push(GamePacket::serialize(&welcome_screen)?);

                    let zone_details_done = TunneledPacket {
                        unknown1: true,
                        inner: ZoneDetailsDone {},
                    };
                    sender_only_packets.push(GamePacket::serialize(&zone_details_done)?);

                    let preload_characters_done = TunneledPacket {
                        unknown1: true,
                        inner: PreloadCharactersDone { unknown1: false },
                    };
                    sender_only_packets.push(GamePacket::serialize(&preload_characters_done)?);

                    broadcasts.push(Broadcast::Single(sender, sender_only_packets));
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
                    // TODO: broadcast pos update to all players
                    broadcasts.append(&mut Zone::move_character(sender, pos_update, self)?);
                }
                OpCode::ZoneTeleportRequest => {
                    let teleport_request: ZoneTeleportRequest =
                        DeserializePacket::deserialize(&mut cursor)?;

                    broadcasts.append(&mut self.lock_enforcer().write_characters(
                        |characters_table_write_handle: &mut GuidTableWriteHandle<
                            u64,
                            Character,
                            CharacterIndex,
                        >,
                         zones_lock_enforcer| {
                            zones_lock_enforcer.read_zones(|zones_table_read_handle| {
                                let possible_instance_guid =
                                    shorten_zone_template_guid(teleport_request.destination_guid)
                                        .and_then(|template_guid| {
                                            GameServer::any_instance(
                                                zones_table_read_handle,
                                                template_guid,
                                            )
                                        });
                                let read_guids = if let Ok(instance_guid) = possible_instance_guid {
                                    vec![instance_guid]
                                } else {
                                    Vec::new()
                                };

                                ZoneLockRequest {
                                    read_guids,
                                    write_guids: Vec::new(),
                                    zone_consumer: move |_, zones_read, _| {
                                        if let Ok(instance_guid) = possible_instance_guid {
                                            teleport_to_zone!(
                                                characters_table_write_handle,
                                                sender,
                                                zones_read.get(&instance_guid).expect(
                                                    "any_instance returned invalid zone GUID"
                                                ),
                                                None,
                                                None,
                                                self.mounts()
                                            )
                                        } else {
                                            Err(ProcessPacketError::CorruptedPacket)
                                        }
                                    },
                                }
                            })
                        },
                    )?);
                }
                OpCode::TeleportToSafety => {
                    let mut packets = self.lock_enforcer().write_characters(|characters_table_write_handle, zones_lock_enforcer| {
                        if let Some((instance_guid, _, _)) = characters_table_write_handle.index(player_guid(sender)) {
                            zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                read_guids: vec![instance_guid],
                                write_guids: Vec::new(),
                                zone_consumer: |_, zones_read, _| {
                                    if let Some(zone) = zones_read.get(&instance_guid) {
                                        let spawn_pos = zone.default_spawn_pos;
                                        let spawn_rot = zone.default_spawn_rot;

                                        teleport_within_zone(sender, spawn_pos, spawn_rot, characters_table_write_handle, &self.mounts)
                                    } else {
                                        println!("Player {} outside zone tried to teleport to safety", sender);
                                        Err(ProcessPacketError::CorruptedPacket)
                                    }
                                },
                            })
                        } else {
                            println!("Unknown player {} tried to teleport to safety", sender);
                            Err(ProcessPacketError::CorruptedPacket)
                        }
                    })?;
                    broadcasts.append(&mut packets);
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
                OpCode::Inventory => {
                    broadcasts.append(&mut process_inventory_packet(self, &mut cursor, sender)?);
                }
                OpCode::BrandishHolster => {
                    self.lock_enforcer().read_characters(|_| CharacterLockRequest {
                        read_guids: Vec::new(),
                        write_guids: vec![player_guid(sender)],
                        character_consumer: |_, _, mut characters_write, _| {
                            if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {
                                character_write_handle.brandish_or_holster();
                                broadcasts.push(Broadcast::Single(sender, vec![
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: QueueAnimation {
                                            character_guid: player_guid(sender),
                                            animation_id: 3300,
                                            queue_pos: 0,
                                            delay_seconds: 0.0,
                                            duration_seconds: 2.0,
                                        }
                                    })?,
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: UpdateWieldType {
                                            guid: player_guid(sender),
                                            wield_type: character_write_handle.wield_type()
                                        }
                                    })?,
                                ]));
                                Ok(())
                            } else {
                                println!("Unknown player {} requested to brandish or holster their weapon", sender);
                                Err(ProcessPacketError::CorruptedPacket)
                            }
                        }
                    })?;
                }
                _ => println!("Unimplemented: {:?}, {:x?}", op_code, data),
            },
            Err(_) => println!("Unknown op code: {}, {:x?}", raw_op_code, data),
        }

        Ok(broadcasts)
    }

    pub fn default_sabers(&self) -> &BTreeMap<u32, DefaultSaber> {
        &self.default_sabers
    }

    pub fn items(&self) -> &BTreeMap<u32, ItemDefinition> {
        &self.items
    }

    pub fn item_classes(&self) -> &ItemClassDefinitions {
        &self.item_classes
    }

    pub fn read_zone_templates(&self) -> &BTreeMap<u8, ZoneTemplate> {
        &self.zone_templates
    }
    pub fn mounts(&self) -> &BTreeMap<u32, MountConfig> {
        &self.mounts
    }

    pub fn lock_enforcer(&self) -> LockEnforcer {
        self.lock_enforcer_source.lock_enforcer()
    }

    pub fn any_instance(
        zones: &ZoneTableReadHandle<'_>,
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

    pub fn zones_by_template(zones: &ZoneTableReadHandle<'_>, template_guid: u8) -> Vec<u64> {
        zones.keys_by_index(template_guid).collect()
    }
}

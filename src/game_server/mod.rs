use std::backtrace::Backtrace;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::io::{Cursor, Error};
use std::num::ParseIntError;
use std::path::Path;
use std::time::{Duration, Instant};
use std::vec;

use byteorder::{LittleEndian, ReadBytesExt};
use handlers::character::{
    Character, CharacterCategory, CharacterType, Chunk, MinigameMatchmakingGroup,
};
use handlers::chat::process_chat_packet;
use handlers::command::process_command;
use handlers::guid::{GuidTable, GuidTableIndexer, IndexedGuid};
use handlers::housing::process_housing_packet;
use handlers::inventory::{
    customizations_from_guids, load_customization_item_mappings, load_customizations,
    load_default_sabers, process_inventory_packet, update_saber_tints, DefaultSaber,
};
use handlers::item::load_item_definitions;
use handlers::lock_enforcer::{
    CharacterLockEnforcer, CharacterLockRequest, CharacterTableWriteHandle, LockEnforcerSource,
    ZoneLockEnforcer, ZoneLockRequest, ZoneTableWriteHandle,
};
use handlers::login::{log_in, log_out, send_points_of_interest};
use handlers::minigame::{
    create_active_minigame_if_uncreated, load_all_minigames, prepare_active_minigame_instance,
    process_minigame_packet, remove_group_from_matchmaking, AllMinigameConfigs,
    MatchmakingGroupStatus,
};
use handlers::mount::{load_mounts, process_mount_packet, MountConfig};
use handlers::reference_data::{load_categories, load_item_classes, load_item_groups};
use handlers::store::{load_cost_map, CostEntry};
use handlers::test_data::make_test_nameplate_image;
use handlers::time::make_game_time_sync;
use handlers::unique_guid::{
    player_guid, shorten_player_guid, shorten_zone_index, zone_instance_guid,
};
use handlers::zone::{
    load_zones, teleport_anywhere, teleport_within_zone, DestinationZoneInstance,
    PointOfInterestConfig, ZoneInstance, ZoneTemplate,
};
use packets::client_update::{Health, Power, PreloadCharactersDone, Stat, StatId, Stats};
use packets::item::ItemDefinition;
use packets::login::{LoginRequest, WelcomeScreen, ZoneDetailsDone};
use packets::player_update::{Customization, InitCustomizations, QueueAnimation, UpdateWieldType};
use packets::reference_data::{CategoryDefinitions, ItemClassDefinitions, ItemGroupDefinitions};
use packets::store::StoreItemList;
use packets::tunnel::{TunneledPacket, TunneledWorldPacket};
use packets::update_position::{PlayerJump, UpdatePlayerPlatformPosition, UpdatePlayerPosition};
use packets::zone::PointOfInterestTeleportRequest;
use packets::{GamePacket, OpCode};
use rand::Rng;

use crate::{info, ConfigError};
use packet_serialize::{DeserializePacket, DeserializePacketError};

mod handlers;
mod packets;

#[derive(Debug)]
pub enum Broadcast {
    Single(u32, Vec<Vec<u8>>),
    Multi(Vec<u32>, Vec<Vec<u8>>),
}

#[non_exhaustive]
#[derive(Debug)]
pub enum ProcessPacketErrorType {
    ConstraintViolated,
    DeserializeError,
    SerializeError,
    UnknownOpCode,
}

pub struct ProcessPacketError {
    backtrace: Backtrace,
    err_type: ProcessPacketErrorType,
    message: String,
}

impl ProcessPacketError {
    pub fn new(err_type: ProcessPacketErrorType, message: String) -> ProcessPacketError {
        ProcessPacketError {
            backtrace: Backtrace::capture(),
            err_type,
            message,
        }
    }
}

impl From<Error> for ProcessPacketError {
    fn from(err: Error) -> Self {
        ProcessPacketError::new(
            ProcessPacketErrorType::DeserializeError,
            format!("IO Error: {}", err),
        )
    }
}

impl From<ParseIntError> for ProcessPacketError {
    fn from(err: ParseIntError) -> Self {
        ProcessPacketError::new(
            ProcessPacketErrorType::DeserializeError,
            format!("Parse int error: {}", err),
        )
    }
}

impl From<DeserializePacketError> for ProcessPacketError {
    fn from(err: DeserializePacketError) -> Self {
        ProcessPacketError::new(
            ProcessPacketErrorType::DeserializeError,
            format!("Deserialize Error: {:?}", err),
        )
    }
}

impl Display for ProcessPacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "{:?}: {}. Backtrace:\n{}",
            self.err_type, self.message, self.backtrace
        ))
    }
}

pub struct GameServer {
    categories: CategoryDefinitions,
    costs: BTreeMap<u32, CostEntry>,
    customizations: BTreeMap<u32, Customization>,
    customization_item_mappings: BTreeMap<u32, Vec<u32>>,
    default_sabers: BTreeMap<u32, DefaultSaber>,
    lock_enforcer_source: LockEnforcerSource,
    items: BTreeMap<u32, ItemDefinition>,
    item_classes: ItemClassDefinitions,
    item_groups: ItemGroupDefinitions,
    minigames: AllMinigameConfigs,
    mounts: BTreeMap<u32, MountConfig>,
    points_of_interest: BTreeMap<u32, (u8, PointOfInterestConfig)>,
    start_time: Instant,
    zone_templates: BTreeMap<u8, ZoneTemplate>,
}

impl GameServer {
    pub fn new(config_dir: &Path) -> Result<Self, ConfigError> {
        let characters = GuidTable::new();
        let (templates, zones, points_of_interest) = load_zones(config_dir)?;
        let item_definitions = load_item_definitions(config_dir)?;
        let item_groups = load_item_groups(config_dir)?;
        Ok(GameServer {
            categories: load_categories(config_dir)?,
            costs: load_cost_map(config_dir, &item_definitions, &item_groups)?,
            customizations: load_customizations(config_dir)?,
            customization_item_mappings: load_customization_item_mappings(config_dir)?,
            default_sabers: load_default_sabers(config_dir)?,
            lock_enforcer_source: LockEnforcerSource::from(characters, zones, GuidTable::new()),
            items: item_definitions,
            item_classes: load_item_classes(config_dir)?,
            item_groups: ItemGroupDefinitions {
                definitions: item_groups,
            },
            minigames: load_all_minigames(config_dir)?,
            mounts: load_mounts(config_dir)?,
            points_of_interest,
            start_time: Instant::now(),
            zone_templates: templates,
        })
    }

    pub fn authenticate(&self, data: Vec<u8>) -> Result<(u32, String), ProcessPacketError> {
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::LoginRequest => {
                    let login_packet: LoginRequest = DeserializePacket::deserialize(&mut cursor)?;
                    shorten_player_guid(login_packet.guid).map(|guid| (guid, login_packet.version))
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Client tried to log in without a login request, data: {:x?}",
                        data
                    ),
                )),
            },
            Err(_) => Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown op code at login: {}", raw_op_code),
            )),
        }
    }

    pub fn log_in(&self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        log_in(sender, self)
    }

    pub fn log_out(&self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        Ok(log_out(sender, self))
    }

    pub fn tick(&self) -> Vec<Broadcast> {
        let now = Instant::now();
        let tickable_characters_by_chunk = self.tickable_characters_by_chunk();

        let mut broadcasts = Vec::new();

        // Lock once for each chunk to avoid holding the lock for an extended period of time,
        // because we are considering all characters in all worlds
        for ((instance_guid, chunk), tickable_characters) in
            tickable_characters_by_chunk.into_iter()
        {
            self.tick_single_chunk(
                now,
                instance_guid,
                chunk,
                tickable_characters,
                &mut broadcasts,
            );
        }

        broadcasts.append(&mut self.tick_minigame_groups());

        broadcasts
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

                    // Set the player as ready
                    self.lock_enforcer()
                        .write_characters(|characters_table_write_handle, _| {
                            characters_table_write_handle.update_value_indices(
                                player_guid(sender),
                                |possible_character_write_handle, _| {
                                    let Some(character_write_handle) =
                                        possible_character_write_handle
                                    else {
                                        return Err(ProcessPacketError::new(
                                            ProcessPacketErrorType::ConstraintViolated,
                                            format!(
                                                "Player {} sent ready packet but does not exist",
                                                sender
                                            ),
                                        ));
                                    };

                                    let CharacterType::Player(ref mut player) =
                                        &mut character_write_handle.stats.character_type
                                    else {
                                        return Err(ProcessPacketError::new(
                                            ProcessPacketErrorType::ConstraintViolated,
                                            format!(
                                            "Character {} sent ready packet but is not a player",
                                            player_guid(sender)
                                        ),
                                        ));
                                    };

                                    if let Some(minigame_status) = &mut player.minigame_status {
                                        broadcasts.append(
                                            &mut create_active_minigame_if_uncreated(
                                                sender,
                                                self.minigames(),
                                                minigame_status,
                                            )?,
                                        );
                                    }

                                    if player.first_load {
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
                                        sender_only_packets
                                            .push(GamePacket::serialize(&welcome_screen));

                                        let minigame_definitions = TunneledPacket {
                                            unknown1: true,
                                            inner: GamePacket::serialize(
                                                &self.minigames.definitions(),
                                            ),
                                        };
                                        sender_only_packets
                                            .push(GamePacket::serialize(&minigame_definitions));
                                    }

                                    player.ready = true;
                                    player.first_load = false;
                                    Ok(())
                                },
                            )
                        })?;

                    sender_only_packets.append(&mut send_points_of_interest(self));

                    let categories = TunneledPacket {
                        unknown1: true,
                        inner: GamePacket::serialize(&self.categories),
                    };
                    sender_only_packets.push(GamePacket::serialize(&categories));

                    let item_groups = TunneledPacket {
                        unknown1: true,
                        inner: GamePacket::serialize(&self.item_groups),
                    };
                    sender_only_packets.push(GamePacket::serialize(&item_groups));

                    let store_items = TunneledPacket {
                        unknown1: true,
                        inner: GamePacket::serialize(&StoreItemList::from(&self.costs)),
                    };
                    sender_only_packets.push(GamePacket::serialize(&store_items));

                    let mut character_broadcasts = self.lock_enforcer().read_characters(|characters_table_read_handle| {
                        let possible_index = characters_table_read_handle.index1(player_guid(sender));
                        let character_diffs = possible_index.map(|(_, instance_guid, chunk)| ZoneInstance::diff_character_guids(
                            instance_guid,
                            Character::MIN_CHUNK,
                            chunk,
                            characters_table_read_handle,
                            player_guid(sender)
                        ))
                            .unwrap_or_default();

                        let read_character_guids: Vec<u64> = character_diffs.character_diffs_for_moved_character.keys().copied().collect();
                        CharacterLockRequest {
                            read_guids: read_character_guids,
                            write_guids: vec![player_guid(sender)],
                            character_consumer: move |characters_table_read_handle, characters_read, mut characters_write, minigame_data_lock_enforcer| {
                                let Some((_, instance_guid, chunk)) = possible_index else {
                                    return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} sent a ready packet but is not in any zone",
                                        sender)));
                                };

                                let zones_lock_enforcer: ZoneLockEnforcer = minigame_data_lock_enforcer.into();
                                zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                    read_guids: vec![instance_guid],
                                    write_guids: Vec::new(),
                                    zone_consumer: |_, zones_read, _| {
                                        let Some(zone) = zones_read.get(&instance_guid) else {
                                            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} sent a ready packet from unknown zone {}",
                                                sender, instance_guid)));
                                        };

                                        let mut sender_only_character_packets = zone.send_self_on_client_ready();

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
                                        sender_only_character_packets.push(GamePacket::serialize(&stats));

                                        let health = TunneledPacket {
                                            unknown1: true,
                                            inner: Health {
                                                current: 25000,
                                                max: 25000,
                                            },
                                        };
                                        sender_only_character_packets.push(GamePacket::serialize(&health));

                                        let power = TunneledPacket {
                                            unknown1: true,
                                            inner: Power {
                                                current: 300,
                                                max: 300,
                                            },
                                        };
                                        sender_only_character_packets.push(GamePacket::serialize(&power));

                                        let mut character_broadcasts = Vec::new();

                                        let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) else {
                                            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} sent a ready packet", sender)));
                                        };

                                        character_write_handle.stats.speed.base = zone.speed;
                                        character_write_handle.stats.jump_height_multiplier.base = zone.jump_height_multiplier;

                                        let mut global_packets = character_write_handle.stats.add_packets(false, self.mounts(), self.items(), self.customizations());
                                        let wield_type = TunneledPacket {
                                            unknown1: true,
                                            inner: UpdateWieldType {
                                                guid: player_guid(sender),
                                                wield_type: character_write_handle.stats.wield_type(),
                                            },
                                        };
                                        global_packets.push(GamePacket::serialize(&wield_type));

                                        if let CharacterType::Player(player) = &character_write_handle.stats.character_type {
                                            sender_only_character_packets.push(GamePacket::serialize(&TunneledPacket {
                                                unknown1: true,
                                                inner: InitCustomizations {
                                                    customizations: customizations_from_guids(player.customizations.values().cloned(), self.customizations()),
                                                },
                                            }));

                                            if let Some(battle_class) = player.battle_classes.get(&player.active_battle_class) {
                                                character_broadcasts.append(&mut update_saber_tints(
                                                    sender,
                                                    characters_table_read_handle,
                                                    instance_guid,
                                                    chunk,
                                                    &battle_class.items,
                                                    player.active_battle_class,
                                                    character_write_handle.stats.wield_type(),
                                                    self
                                                ));
                                            }
                                        }

                                        let all_players_nearby = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle);
                                        character_broadcasts.push(Broadcast::Multi(all_players_nearby, global_packets));

                                        character_broadcasts.push(Broadcast::Single(sender, sender_only_character_packets));
                                        character_broadcasts.append(&mut ZoneInstance::diff_character_broadcasts(player_guid(sender), character_diffs, &characters_read, self.mounts(), self.items(), self.customizations()));

                                        Ok(character_broadcasts)
                                    },
                                })
                            },
                        }
                    })?;
                    broadcasts.append(&mut character_broadcasts);

                    sender_only_packets.append(&mut make_test_nameplate_image(sender));

                    let zone_details_done = TunneledPacket {
                        unknown1: true,
                        inner: ZoneDetailsDone {},
                    };
                    sender_only_packets.push(GamePacket::serialize(&zone_details_done));

                    let preload_characters_done = TunneledPacket {
                        unknown1: true,
                        inner: PreloadCharactersDone { unknown1: false },
                    };
                    sender_only_packets.push(GamePacket::serialize(&preload_characters_done));

                    broadcasts.push(Broadcast::Single(sender, sender_only_packets));
                }
                OpCode::GameTimeSync => {
                    let sender_guid = player_guid(sender);
                    self.lock_enforcer()
                        .read_characters(|_| CharacterLockRequest {
                            read_guids: vec![],
                            write_guids: vec![],
                            character_consumer:
                                |characters_table_read_handle, _, _, minigame_data_lock_enforcer| {
                                    let Some((_, instance_guid, _)) =
                                        characters_table_read_handle.index1(sender_guid)
                                    else {
                                        return Err(ProcessPacketError::new(
                                            ProcessPacketErrorType::ConstraintViolated,
                                            format!("Couldn't sync time for player {} because they are not in any zone", sender_guid)
                                        ));
                                    };

                                    let zones_lock_enforcer: ZoneLockEnforcer =
                                        minigame_data_lock_enforcer.into();
                                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                        read_guids: vec![instance_guid],
                                        write_guids: vec![],
                                        zone_consumer: |_, zones_read, _| {
                                            let Some(zone_read_handle) = zones_read.get(&instance_guid) else {
                                                return Err(ProcessPacketError::new(
                                                    ProcessPacketErrorType::ConstraintViolated,
                                                    format!("Couldn't sync time for player {} because their instance {} does not exist", sender_guid, instance_guid)
                                                ));
                                            };

                                            let game_time_sync = TunneledPacket {
                                                unknown1: true,
                                                inner: make_game_time_sync(
                                                    zone_read_handle.seconds_per_day,
                                                ),
                                            };

                                            broadcasts.push(Broadcast::Single(
                                                sender,
                                                vec![GamePacket::serialize(&game_time_sync)],
                                            ));

                                            Ok::<(), ProcessPacketError>(())
                                        },
                                    })
                                },
                        })?;
                }
                OpCode::Command => {
                    broadcasts.append(&mut process_command(self, sender, &mut cursor)?);
                }
                OpCode::UpdatePlayerPosition => {
                    let mut pos_update: UpdatePlayerPosition =
                        DeserializePacket::deserialize(&mut cursor)?;
                    // Don't allow players to update another player's position
                    pos_update.guid = player_guid(sender);
                    broadcasts.append(&mut ZoneInstance::move_character(pos_update, false, self)?);
                }
                OpCode::PlayerJump => {
                    let mut player_jump: PlayerJump = DeserializePacket::deserialize(&mut cursor)?;
                    // Don't allow players to update another player's position
                    player_jump.pos_update.guid = player_guid(sender);
                    broadcasts.append(&mut ZoneInstance::move_character(player_jump, false, self)?);
                }
                OpCode::UpdatePlayerPlatformPosition => {
                    let mut platform_pos_update: UpdatePlayerPlatformPosition =
                        DeserializePacket::deserialize(&mut cursor)?;
                    // Don't allow players to update another player's position
                    platform_pos_update.pos_update.guid = player_guid(sender);
                    broadcasts.append(&mut ZoneInstance::move_character(
                        platform_pos_update,
                        false,
                        self,
                    )?);
                }
                OpCode::UpdatePlayerCamera => {
                    // Ignore this unused packet to reduce log spam for now
                }
                OpCode::PointOfInterestTeleportRequest => {
                    let teleport_request: PointOfInterestTeleportRequest =
                        DeserializePacket::deserialize(&mut cursor)?;

                    let Some((template_guid, point_of_interest)) = self
                        .points_of_interest()
                        .get(&teleport_request.point_of_interest_guid)
                    else {
                        return Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Player {} requested to teleport to unknown point of interest {}",
                                sender, teleport_request.point_of_interest_guid
                            ),
                        ));
                    };

                    if point_of_interest.teleport_enabled {
                        broadcasts.append(&mut teleport_anywhere(
                            point_of_interest.pos,
                            point_of_interest.rot,
                            DestinationZoneInstance::Any {
                                template_guid: *template_guid,
                            },
                            sender,
                        )?(self)?);
                    }
                }
                OpCode::TeleportToSafety => {
                    let mut packets = self.lock_enforcer().read_characters(|_| {
                        CharacterLockRequest {
                            read_guids: Vec::new(),
                            write_guids: Vec::new(),
                            character_consumer: |characters_table_read_handle, _, _, minigame_data_lock_enforcer| {
                                let Some((_, instance_guid, _)) = characters_table_read_handle.index1(player_guid(sender)) else {
                                    return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} tried to teleport to safety", sender)));
                                };

                                let zones_lock_enforcer: ZoneLockEnforcer = minigame_data_lock_enforcer.into();
                                zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                    read_guids: vec![instance_guid],
                                    write_guids: Vec::new(),
                                    zone_consumer: |_, zones_read, _| {
                                        let Some(zone) = zones_read.get(&instance_guid) else {
                                            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} outside zone tried to teleport to safety", sender)));
                                        };

                                        let spawn_pos = zone.default_spawn_pos;
                                        let spawn_rot = zone.default_spawn_rot;

                                        Ok(teleport_within_zone(sender, spawn_pos, spawn_rot))
                                    },
                                })
                            },
                        }
                    })?;
                    broadcasts.append(&mut packets);
                }
                OpCode::Mount => {
                    broadcasts.append(&mut process_mount_packet(&mut cursor, sender, self)?);
                }
                OpCode::Housing => {
                    broadcasts.append(&mut process_housing_packet(sender, self, &mut cursor)?);
                }
                OpCode::Chat => {
                    broadcasts.append(&mut process_chat_packet(&mut cursor, sender, self)?);
                }
                OpCode::Inventory => {
                    broadcasts.append(&mut process_inventory_packet(self, &mut cursor, sender)?);
                }
                OpCode::BrandishHolster => {
                    self.lock_enforcer().read_characters(|_| CharacterLockRequest {
                        read_guids: Vec::new(),
                        write_guids: vec![player_guid(sender)],
                        character_consumer: |characters_table_read_handle, _, mut characters_write, _| {
                            let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) else {
                                return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} requested to brandish or holster their weapon", sender)));
                            };

                            character_write_handle.brandish_or_holster();

                            let (_, instance_guid, chunk) = character_write_handle.index1();
                            let all_players_nearby = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle);
                            broadcasts.push(Broadcast::Multi(all_players_nearby, vec![
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: QueueAnimation {
                                        character_guid: player_guid(sender),
                                        animation_id: 3300,
                                        queue_pos: 0,
                                        delay_seconds: 0.0,
                                        duration_seconds: 2.0,
                                    }
                                }),
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: UpdateWieldType {
                                        guid: player_guid(sender),
                                        wield_type: character_write_handle.stats.wield_type()
                                    }
                                }),
                            ]));
                            Ok(())
                        }
                    })?;
                }
                OpCode::Logout => {
                    // Allow the cleanup thread to log the player out on disconnect
                }
                OpCode::Minigame => {
                    broadcasts.append(&mut process_minigame_packet(&mut cursor, sender, self)?);
                }
                OpCode::LobbyGameDefinition => {}
                OpCode::UiInteractions => {}
                OpCode::ClientMetrics => {}
                OpCode::ClientLog => {}
                _ => {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::UnknownOpCode,
                        format!("Unimplemented: {:?}, {:x?}", op_code, data),
                    ))
                }
            },
            Err(_) => {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unknown op code: {}, {:x?}", raw_op_code, data),
                ))
            }
        }

        Ok(broadcasts)
    }

    pub fn costs(&self) -> &BTreeMap<u32, CostEntry> {
        &self.costs
    }

    pub fn customizations(&self) -> &BTreeMap<u32, Customization> {
        &self.customizations
    }

    pub fn customization_item_mappings(&self) -> &BTreeMap<u32, Vec<u32>> {
        &self.customization_item_mappings
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

    pub fn minigames(&self) -> &AllMinigameConfigs {
        &self.minigames
    }

    pub fn mounts(&self) -> &BTreeMap<u32, MountConfig> {
        &self.mounts
    }

    pub fn points_of_interest(&self) -> &BTreeMap<u32, (u8, PointOfInterestConfig)> {
        &self.points_of_interest
    }

    pub fn start_time(&self) -> Instant {
        self.start_time
    }

    pub fn lock_enforcer(&self) -> CharacterLockEnforcer {
        self.lock_enforcer_source.lock_enforcer()
    }

    pub fn get_or_create_instance(
        &self,
        characters: &mut CharacterTableWriteHandle<'_>,
        zones: &mut ZoneTableWriteHandle<'_>,
        template_guid: u8,
        required_capacity: u32,
    ) -> Result<u64, ProcessPacketError> {
        let instances = GameServer::unfilled_zones_by_template(
            characters,
            zones,
            template_guid,
            required_capacity,
        );
        if !instances.is_empty() {
            let index = rand::thread_rng().gen_range(0..instances.len());
            return Ok(instances[index]);
        }

        let Some(new_instance_index) = GameServer::find_min_unused_zone_index(zones, template_guid)
        else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("At capacity for zones for template ID {}", template_guid),
            ));
        };

        let Some(template) = self.zone_templates.get(&template_guid) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Tried to teleport to unknown zone template {}",
                    template_guid
                ),
            ));
        };

        if required_capacity > template.max_players {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Zone template {} (capacity {}) does not have the required capacity {}",
                    template_guid, template.max_players, required_capacity
                ),
            ));
        }

        let instance_guid = zone_instance_guid(new_instance_index, template_guid);
        let new_instance = template.to_zone_instance(instance_guid, None, characters);
        zones.insert(new_instance);
        Ok(instance_guid)
    }

    fn unfilled_zones_by_template(
        characters: &CharacterTableWriteHandle<'_>,
        zones: &ZoneTableWriteHandle<'_>,
        template_guid: u8,
        required_capacity: u32,
    ) -> Vec<u64> {
        let unfilled_zones = zones
            .values_by_index1(template_guid)
            .filter_map(|zone_lock| {
                let zone_read_handle = zone_lock.read();
                let instance_guid = zone_read_handle.guid();

                let range = (
                    CharacterCategory::PlayerReady,
                    instance_guid,
                    Character::MIN_CHUNK,
                )
                    ..=(
                        CharacterCategory::PlayerUnready,
                        instance_guid,
                        Character::MAX_CHUNK,
                    );
                let current_players = characters.keys_by_index1_range(range).count() as u32;

                let remaining_capacity =
                    zone_read_handle.max_players.saturating_sub(current_players);

                if required_capacity <= remaining_capacity {
                    Some(instance_guid)
                } else {
                    None
                }
            })
            .collect();

        unfilled_zones
    }

    fn find_min_unused_zone_index(
        zones: &ZoneTableWriteHandle<'_>,
        template_guid: u8,
    ) -> Option<u32> {
        let used_indices: BTreeSet<u32> = zones
            .keys_by_index1(template_guid)
            .map(shorten_zone_index)
            .collect();
        let Some(max) = used_indices.last() else {
            // If we can't simply increment the largest index, just start at the first index
            return Some(0);
        };

        // Only do the expensive operation if we know we can't simply increment the largest index
        if *max == u32::MAX {
            // 0.saturating_sub(0) == 0, so we'll never subtract from 0
            used_indices
                .iter()
                .find(|index| !used_indices.contains(&(*index).saturating_sub(1)))
                .map(|used_index| *used_index - 1)
        } else {
            Some(max + 1)
        }
    }

    fn tickable_characters_by_chunk(&self) -> BTreeMap<(u64, Chunk), Vec<u64>> {
        self.lock_enforcer()
            .read_characters(|characters_table_read_handle| {
                let categories = [
                    CharacterCategory::NpcTickable,
                    CharacterCategory::NpcAutoInteractTickable,
                ];

                let tickable_characters: Vec<u64> = categories
                    .into_iter()
                    .flat_map(|category| {
                        let range = (category, u64::MIN, Character::MIN_CHUNK)
                            ..=(category, u64::MAX, Character::MAX_CHUNK);
                        characters_table_read_handle.keys_by_index1_range(range)
                    })
                    .collect();

                let tickable_characters_by_chunk = tickable_characters.into_iter().fold(
                    BTreeMap::new(),
                    |mut acc: BTreeMap<(u64, Chunk), Vec<u64>>, guid| {
                        // The NPC could have been removed since we last acquired the table read lock
                        if let Some((_, instance_guid, chunk)) =
                            characters_table_read_handle.index1(guid)
                        {
                            acc.entry((instance_guid, chunk)).or_default().push(guid);
                        }
                        acc
                    },
                );

                CharacterLockRequest {
                    read_guids: Vec::new(),
                    write_guids: Vec::new(),
                    character_consumer: |_, _, _, _| tickable_characters_by_chunk,
                }
            })
    }

    fn tick_single_chunk(
        &self,
        now: Instant,
        instance_guid: u64,
        chunk: Chunk,
        tickable_characters: Vec<u64>,
        broadcasts: &mut Vec<Broadcast>,
    ) {
        self.lock_enforcer().read_characters(|characters_table_read_handle| {
            let nearby_player_guids = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle);
            let nearby_players: Vec<u64> = nearby_player_guids.iter()
                .map(|guid| *guid as u64)
                .collect();

            CharacterLockRequest {
                read_guids: nearby_players.clone(),
                write_guids: tickable_characters,
                character_consumer: move |_,
                characters_read,
                mut characters_write,
                _| {

                    // We need to tick characters who update independently first, so that their dependent
                    // characters' previous procedures are not ticked
                    let mut characters_not_updated = Vec::new();
                    for tickable_character in characters_write.values_mut() {
                        if tickable_character.synchronize_with.is_none() {
                            broadcasts.append(
                                &mut tickable_character.tick(now, &nearby_player_guids, &characters_read, self.mounts(), self.items(), self.customizations()),
                            );
                        } else {
                            characters_not_updated.push(tickable_character.guid());
                        }
                    }

                    // Determine which procedures to update in the dependent characters
                    let mut new_procedures = BTreeMap::new();
                    for guid in characters_not_updated.iter() {
                        let tickable_character = characters_write.get(guid).unwrap();
                        if let Some(synchronize_with) = &tickable_character.synchronize_with {
                            if let Some(synchronized_character) =
                                characters_write.get(synchronize_with)
                            {
                                if let Some(synchronized_guid) = synchronized_character.synchronize_with {
                                    panic!(
                                        "Cannot synchronize character {} to a character {} because they are synchronized to character {}",
                                        guid,
                                        synchronized_character.guid(),
                                        synchronized_guid
                                    );
                                }

                                if synchronized_character.last_procedure_change() > tickable_character.last_procedure_change() {
                                    if let Some(key) =
                                        synchronized_character.current_tickable_procedure()
                                    {
                                        new_procedures
                                            .insert(guid, key.clone());
                                    }
                                }
                            }
                        }
                    }

                    // Tick all the dependent characters
                    for guid in characters_not_updated.iter() {
                        let tickable_character = characters_write.get_mut(guid).unwrap();
                        if let Some(key) = new_procedures.remove(&tickable_character.guid()) {
                            tickable_character.set_tickable_procedure_if_exists(key, now);
                        }

                        broadcasts.append(
                            &mut tickable_character.tick(now, &nearby_player_guids, &characters_read, self.mounts(), self.items(), self.customizations()),
                        );
                    }
                },
            }
        })
    }

    fn tick_minigame_groups(&self) -> Vec<Broadcast> {
        let now = Instant::now();
        self.lock_enforcer().write_characters(
            |characters_table_write_handle, minigame_data_lock_enforcer| {
                minigame_data_lock_enforcer.write_minigame_data(|minigame_data_table_write_handle, zones_lock_enforcer| {
                    let mut broadcasts = Vec::new();

                    // Iterate over timed-out groups for every stage, since the number of stages remains
                    // a fairly small constant, while there can theoretically be billions of matchmaking groups.
                    for stage in self.minigames().stage_configs() {
                        let timeout =
                            Duration::from_millis(stage.stage_config.matchmaking_timeout_millis() as u64);
                        // Make sure max time is greater than or equal to start time so that the range is valid
                        let max_time = match now.checked_sub(timeout) {
                            Some(max_time) => max_time,
                            None => continue,
                        }.max(self.start_time);
                        let stage_group_guid = stage.stage_group_guid;
                        let stage_guid = stage.stage_config.guid();
                        let min_players = stage.stage_config.min_players();

                        let timed_out_group_range = (MatchmakingGroupStatus::Open, stage_guid, self.start_time)..=(MatchmakingGroupStatus::Open, stage_guid, max_time);
                        let timed_out_groups: Vec<MinigameMatchmakingGroup> = minigame_data_table_write_handle
                            .keys_by_index2_range(timed_out_group_range)
                            .collect();
                        for matchmaking_group in timed_out_groups {
                            let players_in_group: Vec<u32> = characters_table_write_handle
                                .keys_by_index4(&matchmaking_group)
                                .filter_map(|guid| shorten_player_guid(guid).ok())
                                .collect();

                            zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                                if players_in_group.len() as u32 >= min_players {
                                    broadcasts.append(&mut prepare_active_minigame_instance(
                                        matchmaking_group,
                                        &players_in_group,
                                        &stage,
                                        characters_table_write_handle,
                                        minigame_data_table_write_handle,
                                        zones_table_write_handle,
                                        None,
                                        self,
                                    ));
                                    return;
                                }

                                if players_in_group.len() == 1 {
                                    if let Some(replacement_stage_locator) = &stage.stage_config.single_player_stage_guid() {
                                        if let Some(replacement_stage) = self
                                            .minigames()
                                            .stage_config(replacement_stage_locator.stage_group_guid, replacement_stage_locator.stage_guid)
                                        {
                                            if replacement_stage.stage_config.min_players() == 1 {
                                                broadcasts.append(&mut prepare_active_minigame_instance(
                                                    matchmaking_group,
                                                    &players_in_group,
                                                    &replacement_stage,
                                                    characters_table_write_handle,
                                                    minigame_data_table_write_handle,
                                                    zones_table_write_handle,
                                                    Some(44218),
                                                    self,
                                                ));
                                                return;
                                            } else {
                                                info!(
                                                    "Replacement stage (stage group {}, stage {}) for (stage group {}, stage {}) isn't single-player",
                                                    replacement_stage_locator.stage_group_guid,
                                                    replacement_stage_locator.stage_guid,
                                                    stage_group_guid,
                                                    stage_guid
                                                );
                                            }
                                        } else {
                                            info!(
                                                "Couldn't find replacement stage (stage group {}, stage {}) for (stage group {}, stage {})",
                                                replacement_stage_locator.stage_group_guid,
                                                replacement_stage_locator.stage_guid,
                                                stage_group_guid,
                                                stage_guid
                                            );
                                        }
                                    }
                                }

                                broadcasts.append(&mut remove_group_from_matchmaking(
                                    &players_in_group,
                                    &BTreeSet::new(),
                                    matchmaking_group,
                                    characters_table_write_handle,
                                    minigame_data_table_write_handle,
                                    zones_table_write_handle,
                                    Some(33781),
                                    self,
                                ));
                            })
                        }
                    }

                    broadcasts
                })
            },
        )
    }
}

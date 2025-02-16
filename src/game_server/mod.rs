use std::backtrace::Backtrace;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::io::{Cursor, Error};
use std::num::ParseIntError;
use std::path::Path;
use std::time::Instant;
use std::vec;

use byteorder::{LittleEndian, ReadBytesExt};
use handlers::character::{
    Character, CharacterCategory, CharacterLocationIndex, CharacterNameIndex, CharacterSquadIndex,
    CharacterType, Chunk,
};
use handlers::chat::process_chat_packet;
use handlers::command::process_command;
use handlers::guid::{GuidTable, GuidTableIndexer, GuidTableWriteHandle, IndexedGuid};
use handlers::housing::process_housing_packet;
use handlers::inventory::{
    customizations_from_guids, load_customization_item_mappings, load_customizations,
    load_default_sabers, process_inventory_packet, update_saber_tints, DefaultSaber,
};
use handlers::item::load_item_definitions;
use handlers::lock_enforcer::{
    CharacterLockRequest, CharacterTableWriteHandle, LockEnforcer, LockEnforcerSource,
    ZoneLockRequest, ZoneTableWriteHandle,
};
use handlers::login::{log_in, log_out, send_points_of_interest};
use handlers::minigame::{
    create_active_minigame, load_all_minigames, process_minigame_packet, AllMinigameConfigs,
};
use handlers::mount::{load_mounts, process_mount_packet, MountConfig};
use handlers::reference_data::{load_categories, load_item_classes, load_item_groups};
use handlers::store::{load_cost_map, CostEntry};
use handlers::test_data::make_test_nameplate_image;
use handlers::time::make_game_time_sync;
use handlers::unique_guid::{
    player_guid, shorten_player_guid, shorten_zone_index, shorten_zone_template_guid,
    zone_instance_guid,
};
use handlers::zone::{load_zones, teleport_within_zone, ZoneInstance, ZoneTemplate};
use packets::ability::{AbilityDefinition, LaunchAndLand, TargetArray, UnknownAbilityDefArray};
use packets::client_update::{Health, Power, PreloadCharactersDone, Stat, StatId, Stats};
use packets::housing::{HouseDescription, HouseInstanceEntry, HouseInstanceList};
use packets::item::ItemDefinition;
use packets::login::{LoginRequest, WelcomeScreen, ZoneDetailsDone};
use packets::player_update::{
    Customization, InitCustomizations, PlayCompositeEffect, QueueAnimation, SetLookAt,
    UpdateWieldType,
};
use packets::reference_data::{CategoryDefinitions, ItemClassDefinitions, ItemGroupDefinitions};
use packets::store::StoreItemList;
use packets::tunnel::{TunneledPacket, TunneledWorldPacket};
use packets::update_position::{PlayerJump, UpdatePlayerPlatformPosition, UpdatePlayerPosition};
use packets::zone::ZoneTeleportRequest;
use packets::{CharacterBoneNameTarget, GamePacket, GuidTarget, OpCode, Pos, Target};
use rand::Rng;

use crate::{info, teleport_to_zone};
use packet_serialize::{DeserializePacket, DeserializePacketError, SerializePacketError};

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

impl From<SerializePacketError> for ProcessPacketError {
    fn from(err: SerializePacketError) -> Self {
        ProcessPacketError::new(
            ProcessPacketErrorType::SerializeError,
            format!("Serialize Error: {:?}", err),
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
    zone_templates: BTreeMap<u8, ZoneTemplate>,
}

impl GameServer {
    pub fn new(config_dir: &Path) -> Result<Self, Error> {
        let characters = GuidTable::new();
        let (templates, zones) = load_zones(config_dir)?;
        let item_definitions = load_item_definitions(config_dir)?;
        let item_groups = load_item_groups(config_dir)?;
        Ok(GameServer {
            categories: load_categories(config_dir)?,
            costs: load_cost_map(config_dir, &item_definitions, &item_groups)?,
            customizations: load_customizations(config_dir)?,
            customization_item_mappings: load_customization_item_mappings(config_dir)?,
            default_sabers: load_default_sabers(config_dir)?,
            lock_enforcer_source: LockEnforcerSource::from(characters, zones),
            items: item_definitions,
            item_classes: load_item_classes(config_dir)?,
            item_groups: ItemGroupDefinitions {
                definitions: item_groups,
            },
            minigames: load_all_minigames(config_dir)?,
            mounts: load_mounts(config_dir)?,
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
        log_out(sender, self)
    }

    pub fn tick(&self) -> Result<Vec<Broadcast>, ProcessPacketError> {
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
            )?;
        }

        Ok(broadcasts)
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
                            match characters_table_write_handle.remove(player_guid(sender)) {
                                Some((character, _, _, _)) => {
                                    let mut character_write_handle = character.write();
                                    if let CharacterType::Player(ref mut player) =
                                        &mut character_write_handle.stats.character_type
                                    {
                                        if let Some(minigame_status) = &mut player.minigame_status {
                                            if !minigame_status.game_created {
                                                minigame_status.game_created = true;
                                                broadcasts.append(&mut create_active_minigame(
                                                    sender,
                                                    self.minigames(),
                                                    minigame_status,
                                                )?);
                                            }
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
                                                .push(GamePacket::serialize(&welcome_screen)?);

                                            let minigame_definitions = TunneledPacket {
                                                unknown1: true,
                                                inner: GamePacket::serialize(
                                                    &self.minigames.definitions(),
                                                )?,
                                            };
                                            sender_only_packets.push(GamePacket::serialize(
                                                &minigame_definitions,
                                            )?);
                                        }

                                        player.ready = true;
                                        player.first_load = false;
                                    }
                                    let guid = character_write_handle.guid();
                                    let new_index1 = character_write_handle.index1();
                                    let new_index2 = character_write_handle.index2();
                                    let new_index3 = character_write_handle.index3();
                                    drop(character_write_handle);
                                    characters_table_write_handle.insert_lock(
                                        guid, new_index1, new_index2, new_index3, character,
                                    );
                                    Ok(())
                                }
                                None => Err(ProcessPacketError::new(
                                    ProcessPacketErrorType::ConstraintViolated,
                                    format!(
                                        "Player {} sent ready packet but is not in any zone",
                                        sender
                                    ),
                                )),
                            }
                        })?;

                    sender_only_packets.append(&mut send_points_of_interest(self)?);

                    let categories = TunneledPacket {
                        unknown1: true,
                        inner: GamePacket::serialize(&self.categories)?,
                    };
                    sender_only_packets.push(GamePacket::serialize(&categories)?);

                    let item_groups = TunneledPacket {
                        unknown1: true,
                        inner: GamePacket::serialize(&self.item_groups)?,
                    };
                    sender_only_packets.push(GamePacket::serialize(&item_groups)?);

                    let store_items = TunneledPacket {
                        unknown1: true,
                        inner: GamePacket::serialize(&StoreItemList::from(&self.costs))?,
                    };
                    sender_only_packets.push(GamePacket::serialize(&store_items)?);

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
                            character_consumer: move |characters_table_read_handle, characters_read, mut characters_write, zones_lock_enforcer| {
                                if let Some((_, instance_guid, chunk)) = possible_index {
                                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                        read_guids: vec![instance_guid],
                                        write_guids: Vec::new(),
                                        zone_consumer: |_, zones_read, _| {
                                            if let Some(zone) = zones_read.get(&instance_guid) {
                                                let mut sender_only_character_packets = Vec::new();
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
                                                sender_only_character_packets.push(GamePacket::serialize(&stats)?);

                                                let health = TunneledPacket {
                                                    unknown1: true,
                                                    inner: Health {
                                                        current: 25000,
                                                        max: 25000,
                                                    },
                                                };
                                                sender_only_character_packets.push(GamePacket::serialize(&health)?);

                                                let power = TunneledPacket {
                                                    unknown1: true,
                                                    inner: Power {
                                                        current: 300,
                                                        max: 300,
                                                    },
                                                };
                                                sender_only_character_packets.push(GamePacket::serialize(&power)?);

                                                let mut character_broadcasts = Vec::new();

                                                if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {
                                                    character_write_handle.stats.speed.base = zone.speed;
                                                    character_write_handle.stats.jump_height_multiplier.base = zone.jump_height_multiplier;

                                                    let mut global_packets = character_write_handle.add_packets(self.mounts(), self.items(), self.customizations())?;
                                                    let wield_type = TunneledPacket {
                                                        unknown1: true,
                                                        inner: UpdateWieldType {
                                                            guid: player_guid(sender),
                                                            wield_type: character_write_handle.wield_type(),
                                                        },
                                                    };
                                                    global_packets.push(GamePacket::serialize(&wield_type)?);

                                                    if let CharacterType::Player(player) = &character_write_handle.stats.character_type {
                                                        sender_only_character_packets.push(GamePacket::serialize(&TunneledPacket {
                                                            unknown1: true,
                                                            inner: InitCustomizations {
                                                                customizations: customizations_from_guids(player.customizations.values().cloned(), self.customizations()),
                                                            },
                                                        })?);

                                                        if let Some(battle_class) = player.battle_classes.get(&player.active_battle_class) {
                                                            character_broadcasts.append(&mut update_saber_tints(
                                                                sender,
                                                                characters_table_read_handle,
                                                                instance_guid,
                                                                chunk,
                                                                &battle_class.items,
                                                                player.active_battle_class,
                                                                character_write_handle.wield_type(),
                                                                self
                                                            )?);
                                                        }
                                                    }

                                                    let all_players_nearby = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle)?;
                                                    character_broadcasts.push(Broadcast::Multi(all_players_nearby, global_packets));
                                                } else {
                                                    return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} sent a ready packet", sender)));
                                                }

                                                character_broadcasts.push(Broadcast::Single(sender, sender_only_character_packets));
                                                character_broadcasts.append(&mut ZoneInstance::diff_character_broadcasts(player_guid(sender), character_diffs, &characters_read, self.mounts(), self.items(), self.customizations())?);

                                                Ok(character_broadcasts)
                                            } else {
                                                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} sent a ready packet from unknown zone {}",
                                                    sender, instance_guid)))
                                            }
                                        },
                                    })
                                } else {
                                    Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} sent a ready packet but is not in any zone",
                                        sender)))
                                }
                            },
                        }
                    })?;
                    broadcasts.append(&mut character_broadcasts);

                    sender_only_packets.append(&mut make_test_nameplate_image(sender)?);

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
                    let sender_guid = player_guid(sender);
                    self.lock_enforcer()
                        .read_characters(|_| CharacterLockRequest {
                            read_guids: vec![],
                            write_guids: vec![],
                            character_consumer:
                                |characters_table_read_handle, _, _, zones_lock_enforcer| {
                                    if let Some((_, instance_guid, _)) =
                                        characters_table_read_handle.index1(sender_guid)
                                    {
                                        zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                            read_guids: vec![instance_guid],
                                            write_guids: vec![],
                                            zone_consumer: |_, zones_read, _| {
                                                if let Some(zone_read_handle) =
                                                    zones_read.get(&instance_guid)
                                                {
                                                    let game_time_sync = TunneledPacket {
                                                        unknown1: true,
                                                        inner: make_game_time_sync(
                                                            zone_read_handle.seconds_per_day,
                                                        ),
                                                    };

                                                    broadcasts.push(Broadcast::Single(
                                                        sender,
                                                        vec![GamePacket::serialize(
                                                            &game_time_sync,
                                                        )?],
                                                    ));
                                                }

                                                Ok::<(), ProcessPacketError>(())
                                            },
                                        })
                                    } else {
                                        Ok(())
                                    }
                                },
                        })?;
                }
                OpCode::Command => {
                    broadcasts.append(&mut process_command(self, &mut cursor)?);
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
                OpCode::ZoneTeleportRequest => {
                    let teleport_request: ZoneTeleportRequest =
                        DeserializePacket::deserialize(&mut cursor)?;

                    broadcasts.append(&mut self.lock_enforcer().write_characters(
                        |characters_table_write_handle: &mut GuidTableWriteHandle<
                            u64,
                            Character,
                            CharacterLocationIndex,
                            CharacterNameIndex,
                            CharacterSquadIndex,
                        >,
                         zones_lock_enforcer| {
                            zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                                let possible_instance_guid =
                                    shorten_zone_template_guid(teleport_request.destination_guid)
                                        .and_then(|template_guid| {
                                            self.get_or_create_instance(
                                                characters_table_write_handle,
                                                zones_table_write_handle,
                                                template_guid,
                                                1,
                                            )
                                        });

                                match possible_instance_guid {
                                    Ok(instance_guid) => teleport_to_zone!(
                                        characters_table_write_handle,
                                        sender,
                                        zones_table_write_handle,
                                        &zones_table_write_handle
                                            .get(instance_guid)
                                            .expect(
                                                "get_or_create_instance returned invalid zone GUID"
                                            )
                                            .read(),
                                        None,
                                        None,
                                        self.mounts(),
                                    ),
                                    Err(err) => Err(err),
                                }
                            })
                        },
                    )?);
                }
                OpCode::TeleportToSafety => {
                    let mut packets = self.lock_enforcer().read_characters(|_| {
                        CharacterLockRequest {
                            read_guids: Vec::new(),
                            write_guids: Vec::new(),
                            character_consumer: |characters_table_read_handle, _, _, zones_lock_enforcer| {
                                if let Some((_, instance_guid, _)) = characters_table_read_handle.index1(player_guid(sender)) {
                                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                        read_guids: vec![instance_guid],
                                        write_guids: Vec::new(),
                                        zone_consumer: |_, zones_read, _| {
                                            if let Some(zone) = zones_read.get(&instance_guid) {
                                                let spawn_pos = zone.default_spawn_pos;
                                                let spawn_rot = zone.default_spawn_rot;

                                                teleport_within_zone(sender, spawn_pos, spawn_rot)
                                            } else {
                                                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} outside zone tried to teleport to safety", sender)))
                                            }
                                        },
                                    })
                                } else {
                                    Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} tried to teleport to safety", sender)))
                                }
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
                            if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {
                                character_write_handle.brandish_or_holster();

                                let (_, instance_guid, chunk) = character_write_handle.index1();
                                let all_players_nearby = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle)?;
                                broadcasts.push(Broadcast::Multi(all_players_nearby, vec![
                                    /*GamePacket::serialize(&TunneledPacket {
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
                                    })?,*/
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: AbilityDefinition {
                                            unknown1: 6256,
                                            unknown2: true,
                                            unknown3: false,
                                            unknown4: 5122896,
                                            unknown5: 5122897,
                                            unknown6: 2823,
                                            unknown7: 0.0,
                                            unknown8: 0,
                                            unknown9: 0,
                                            unknown10: 15187,
                                            unknown11: 0,
                                            unknown12: 1099,
                                            unknown13: 1120,
                                            unknown14: 0,
                                            unknown15: 4,
                                            unknown16: 1,
                                            unknown17: 10.0,
                                            unknown18: 10.0,
                                            unknown19: 0,
                                            unknown20: 0,
                                            unknown21: 0,
                                            unknown22: false,
                                            unknown23: 0,
                                            unknown24: 0,
                                            unknown25: 0,
                                            unknown26: 0.0,
                                            unknown27: 0,
                                            unknown28: 5122897,
                                            unknown29: 0.0,
                                            unknown30: 0.0,
                                            unknown31: 0,
                                            unknown32: 0,
                                            unknown33: 0,
                                            unknown34: 0.0,
                                            unknown35: 0.0,
                                            unknown36: false,
                                            unknown37: 0,
                                            unknown38: 0,
                                            unknown39: false,
                                            unknown40: false,
                                            unknown41: false,
                                            unknown42: 0,
                                            unknown43: 0.0,
                                            unknown44: vec![UnknownAbilityDefArray {
                                                unknown1: 5,
                                                unknown2: 61575,
                                                unknown3: 61575,
                                                unknown4: 1,
                                                unknown5: 6256,
                                                unknown6: false,
                                                unknown7: 0,
                                                unknown8: 0,
                                                unknown9: 3.0,
                                                unknown10: 3.0,
                                                unknown11: 3.0,
                                                unknown12: 0,
                                                unknown13: 0,
                                                unknown14: 0,
                                                unknown15: 0,
                                                unknown16: 0,
                                                unknown17: 0,
                                                unknown18: false,
                                                unknown19: false,
                                                unknown20: "".to_string(),
                                                unknown21: 0.0,
                                                unknown22: 0.0,
                                                unknown23: 0.0,
                                                unknown24: 0.0,
                                                unknown25: 0.0,
                                                unknown26: 0.0,
                                                unknown27: 0,
                                                unknown28: 0,
                                                unknown29: 0,
                                                unknown30: 0.0,
                                                unknown31: 0,
                                                unknown32: 0,
                                                unknown33: 0,
                                                unknown34: 0,
                                                unknown35: 0,
                                                unknown36: 0,
                                                unknown37: 0,
                                                unknown38: 0,
                                                unknown39: false,
                                                unknown40: 0.0,
                                                unknown41: 0.0,
                                                unknown42: 0.0,
                                                unknown43: false,
                                            }],
                                        }
                                    })?,
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: SetLookAt {
                                            looker_guid: 1152945693862658072,
                                            target_guid: 1
                                        }
                                    })?,
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: PlayCompositeEffect {
                                            guid: 1,
                                            triggered_by_guid: 1152945693862658072,
                                            composite_effect: 1166,
                                            delay_millis: 0,
                                            duration_millis: 4000,
                                            pos: Pos::default()
                                        }
                                    })?,
                                    GamePacket::serialize(&TunneledPacket { // Test Particle Beam
                                        unknown1: true,
                                        inner: LaunchAndLand {
                                            launcher_guid: 1152945693862658072,
                                            target1: vec![TargetArray {
                                                target: Target::Guid(GuidTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                guid: 1,
                                             })}],
                                            unknown1: 3,
                                            unknown2: 5,
                                            launcher_animation_id: 1001,
                                            launcher_composite_effect: 1166,
                                            unknown5: 0,
                                            unknown6: true,
                                            unknown7: true,
                                            target_animation_unknown1: 0,
                                            unknown9: 0,
                                            unknown10: 0,
                                            unknown11: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            launcher_composite_effect_duration: 0.0,
                                            unknown13: 10.0,
                                            unknown14: 0,
                                            unknown15: 0,
                                            unknown16: 0,
                                            unknown17: 0,
                                            override_launcher_guid: 0,
                                            track_target: true,
                                            unknown20: 0,
                                            unknown21: 0,
                                            progressive_start_speed: 1.0,
                                            progressive_end_speed: 5.0,
                                            unknown24: 0,
                                            unknown25: 0,
                                            unknown26: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown27: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            projectile_adr_name: "".to_string(),
                                            projectile_bone_source: Target::CharacterBone(CharacterBoneNameTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                character_guid: 0,
                                                bone_name: "spineUpper".to_string(),
                                             }),
                                             target3: Target::CharacterBone(CharacterBoneNameTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                character_guid: 0,
                                                bone_name: "".to_string(),
                                             }),
                                            unknown29: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown30: 4.0,
                                            unknown31: false, // == true Targets NPC's feet?
                                            projectile_size: 1.0,
                                            progressive_inflation_size: 0.0,
                                            trail_composite_effect: 0,
                                            landed_composite_effect: 1,
                                            unknown36: 0,
                                            unknown37: 0,
                                            unknown38: 0.0,
                                            projectile_duration_seconds: 0.0,
                                            unknown40: 0.0,
                                            unknown41: 0.0,
                                            unknown42: 0.0,
                                            unknown43: 0.0,
                                            unknown44: 0.0,
                                            unknown45: 0.0,
                                            unknown46: "".to_string(),
                                            unknown47: 0,
                                        }
                                    })?,
                                    /*GamePacket::serialize(&TunneledPacket { // Pistol projectile
                                        unknown1: true,
                                        inner: LaunchAndLand {
                                            launcher_guid: 1,
                                            target1: vec![TargetArray {
                                                target: Target::Guid(GuidTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                guid: 1152945693862658072,
                                             })}],
                                            unknown1: 3,
                                            unknown2: 5,
                                            launcher_animation_id: 1521,//3011,
                                            launcher_composite_effect: 2579,//1101,
                                            unknown5: 0,
                                            unknown6: false,
                                            unknown7: false,
                                            target_animation_unknown1: 3101,
                                            unknown9: 0,
                                            unknown10: 0,
                                            unknown11: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            launcher_composite_effect_duration: 0.0,
                                            unknown13: 0.0,
                                            unknown14: 0,
                                            unknown15: 0,
                                            unknown16: 0,
                                            unknown17: 0,
                                            override_launcher_guid: 0,
                                            track_target: true,
                                            unknown20: 0,
                                            unknown21: 0,
                                            progressive_start_speed: 1.0,
                                            progressive_end_speed: 5.0,
                                            unknown24: 0,
                                            unknown25: 0,
                                            unknown26: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown27: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            projectile_adr_name: "CMBT_BlasterTracer_Pill_Pistol_Blue.adr".to_string(),
                                            projectile_bone_source: Target::CharacterBone(CharacterBoneNameTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                character_guid: 1,
                                                bone_name: "spineUpper".to_string(),
                                             }),
                                             target3: Target::CharacterBone(CharacterBoneNameTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                character_guid: 0,
                                                bone_name: "".to_string(),
                                             }),
                                            unknown29: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown30: 4.0,
                                            unknown31: false, // == true Targets NPC's feet?
                                            projectile_size: 1.0,
                                            progressive_inflation_size: 0.0,
                                            trail_composite_effect: 0,
                                            landed_composite_effect: 1,
                                            unknown36: 0,
                                            unknown37: 0,
                                            unknown38: 0.0,
                                            projectile_duration_seconds: 0.0,
                                            unknown40: 0.0,
                                            unknown41: 0.0,
                                            unknown42: 0.0,
                                            unknown43: 0.0,
                                            unknown44: 0.0,
                                            unknown45: 0.0,
                                            unknown46: "".to_string(),
                                            unknown47: 0,
                                        }
                                    })?,*/
                                    /*GamePacket::serialize(&TunneledPacket { // Grenade
                                        unknown1: true,
                                        inner: LaunchAndLand {
                                            launcher_guid: 1,
                                            target1: vec![TargetArray {
                                                target: Target::Guid(GuidTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                guid: 1152945693862658072,
                                             })}],
                                            unknown1: 3,
                                            unknown2: 5,
                                            launcher_animation_id: 1728,//3011,
                                            launcher_composite_effect: 0,//1101,
                                            unknown5: 0,
                                            unknown6: false,
                                            unknown7: false,
                                            target_animation_unknown1: 1728,
                                            unknown9: 0,
                                            unknown10: 0,
                                            unknown11: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            launcher_composite_effect_duration: 0.0,
                                            unknown13: 0.0,
                                            unknown14: 0,
                                            unknown15: 0,
                                            unknown16: 0,
                                            unknown17: 0,
                                            override_launcher_guid: 0,
                                            track_target: true,
                                            unknown20: 0,
                                            unknown21: 0,
                                            progressive_start_speed: 1.0,
                                            progressive_end_speed: 5.0,
                                            unknown24: 0,
                                            unknown25: 0,
                                            unknown26: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown27: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            projectile_adr_name: "Projectile_Thermal_Grenade.adr".to_string(),
                                            projectile_bone_source: Target::CharacterBone(CharacterBoneNameTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                character_guid: 1,
                                                bone_name: "spineUpper".to_string(),
                                             }),
                                             target3: Target::CharacterBone(CharacterBoneNameTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                character_guid: 0,
                                                bone_name: "".to_string(),
                                             }),
                                            unknown29: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown30: 4.0,
                                            unknown31: false, // == true Targets NPC's feet?
                                            projectile_size: 1.0,
                                            progressive_inflation_size: 0.0,
                                            trail_composite_effect: 0,
                                            landed_composite_effect: 1,
                                            unknown36: 0,
                                            unknown37: 0,
                                            unknown38: 0.0,
                                            projectile_duration_seconds: 0.0,
                                            unknown40: 0.0,
                                            unknown41: 0.0,
                                            unknown42: 0.0,
                                            unknown43: 0.0,
                                            unknown44: 0.0,
                                            unknown45: 0.0,
                                            unknown46: "".to_string(),
                                            unknown47: 0,
                                        }
                                    })?,*/
                                    /*GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: LaunchAndLand {
                                            launcher_guid: 1152945693862658072,
                                            target1: vec![TargetArray {
                                                target: Target::Guid(GuidTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                guid: 1,
                                             })}],
                                            unknown1: -1,
                                            unknown2: 23,
                                            launcher_animation_id: 0,//3011,
                                            launcher_composite_effect: 0,//1101,
                                            unknown5: 0,
                                            unknown6: false,
                                            unknown7: false,
                                            target_animation_unknown1: 0,
                                            unknown9: 5236,
                                            unknown10: 0,
                                            unknown11: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            launcher_composite_effect_duration: 0.0,
                                            unknown13: 1.0,
                                            unknown14: 32563,
                                            unknown15: 2,
                                            unknown16: 5,
                                            unknown17: 0,
                                            override_launcher_guid: 0,
                                            track_target: true,
                                            unknown20: 0,
                                            unknown21: 0,
                                            progressive_start_speed: 1.0,
                                            progressive_end_speed: 1.0,
                                            unknown24: 0,
                                            unknown25: 0,
                                            unknown26: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown27: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            projectile_adr_name: "Projectile_Thermal_Grenade.adr".to_string(),
                                            projectile_bone_source: Target::Guid(GuidTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                guid: 1152945693862658072,
                                             }),
                                             target3: Target::Guid(GuidTarget {
                                                fallback_pos: Pos {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    z: 0.0,
                                                    w: 1.0,
                                                },
                                                guid: 1152945693862658072,
                                             }),
                                            unknown29: Pos {
                                                x: 0.0,
                                                y: 0.0,
                                                z: 0.0,
                                                w: 1.0,
                                            },
                                            unknown30: 0.0,
                                            unknown31: false, // == true Targets NPC's feet?
                                            projectile_size: 1.0,
                                            progressive_inflation_size: 0.0,
                                            trail_composite_effect: 0,
                                            landed_composite_effect: 1,
                                            unknown36: 0,
                                            unknown37: 0,
                                            unknown38: 0.0,
                                            projectile_duration_seconds: 0.0,
                                            unknown40: 0.0,
                                            unknown41: 0.0,
                                            unknown42: 0.0,
                                            unknown43: 0.0,
                                            unknown44: 0.0,
                                            unknown45: 0.0,
                                            unknown46: "".to_string(),
                                            unknown47: 0,
                                        }
                                    })?,*/
                                ]));
                                Ok(())
                            } else {
                                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} requested to brandish or holster their weapon", sender)))
                            }
                        }
                    })?;
                }
                OpCode::Logout => {
                    // Allow the cleanup thread to log the player out on disconnect
                }
                OpCode::Minigame => {
                    broadcasts.append(&mut process_minigame_packet(&mut cursor, sender, self)?);
                }
                _ => info!("Unimplemented: {:?}, {:x?}", op_code, data),
            },
            Err(_) => info!("Unknown op code: {}, {:x?}", raw_op_code, data),
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

    pub fn lock_enforcer(&self) -> LockEnforcer {
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
            Ok(instances[index])
        } else if let Some(new_instance_index) =
            GameServer::find_min_unused_zone_index(zones, template_guid)
        {
            if let Some(template) = self.zone_templates.get(&template_guid) {
                if required_capacity <= template.max_players {
                    let instance_guid = zone_instance_guid(new_instance_index, template_guid);
                    let new_instance = template.to_zone_instance(instance_guid, None, characters);
                    zones.insert(new_instance);
                    Ok(instance_guid)
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Zone template {} (capacity {}) does not have the required capacity {}",
                            template_guid, template.max_players, required_capacity
                        ),
                    ))
                }
            } else {
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to teleport to unknown zone template {}",
                        template_guid
                    ),
                ))
            }
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("At capacity for zones for template ID {}", template_guid),
            ))
        }
    }

    fn unfilled_zones_by_template(
        characters: &CharacterTableWriteHandle<'_>,
        zones: &ZoneTableWriteHandle<'_>,
        template_guid: u8,
        required_capacity: u32,
    ) -> Vec<u64> {
        let unfilled_zones = zones
            .values_by_index(template_guid)
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
        if let Some(max) = used_indices.last() {
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
        } else {
            // Otherwise, just start at the first index
            Some(0)
        }
    }

    fn tickable_characters_by_chunk(&self) -> BTreeMap<(u64, Chunk), Vec<u64>> {
        self.lock_enforcer()
            .read_characters(|characters_table_read_handle| {
                let range = (
                    CharacterCategory::NpcTickable,
                    u64::MIN,
                    Character::MIN_CHUNK,
                )
                    ..=(
                        CharacterCategory::NpcTickable,
                        u64::MAX,
                        Character::MAX_CHUNK,
                    );
                let tickable_characters: Vec<u64> = characters_table_read_handle
                    .keys_by_index1_range(range)
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
    ) -> Result<(), ProcessPacketError> {
        self.lock_enforcer().read_characters(|characters_table_read_handle| {
            let nearby_player_guids = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle)
                .unwrap_or_default();
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
                                &mut tickable_character.tick(now, &nearby_player_guids, &characters_read)?,
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
                            &mut tickable_character.tick(now, &nearby_player_guids, &characters_read)?,
                        );
                    }

                    Ok(())
                },
            }
        })
    }
}

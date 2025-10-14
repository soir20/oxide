use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    iter,
    path::Path,
};

use enum_iterator::all;
use parking_lot::RwLockReadGuard;
use serde::Deserialize;

use crate::{
    game_server::{
        packets::{
            client_update::Position,
            command::MoveToInteract,
            housing::BuildArea,
            item::{ItemDefinition, WieldType},
            login::{ClientBeginZoning, ZoneDetails},
            player_update::Customization,
            tunnel::TunneledPacket,
            ui::{ExecuteScriptWithIntParams, ExecuteScriptWithStringParams},
            GamePacket, Pos,
        },
        Broadcast, GameServer, LogLevel, ProcessPacketError, ProcessPacketErrorType,
        TickableNpcSynchronization,
    },
    info, ConfigError,
};

use super::{
    character::{
        coerce_to_broadcast_supplier, AmbientNpcConfig, Character, CharacterCategory,
        CharacterLocationIndex, CharacterMatchmakingGroupIndex, CharacterNameIndex,
        CharacterSquadIndex, CharacterSynchronizationIndex, CharacterType, Chunk, DoorConfig,
        NpcTemplate, PreviousFixture, PreviousLocation, RemovalMode, TransportConfig,
    },
    distance3,
    guid::{Guid, GuidTable, GuidTableIndexer, GuidTableWriteHandle, IndexedGuid},
    housing::prepare_init_house_packets,
    lock_enforcer::{
        CharacterLockRequest, CharacterReadGuard, CharacterTableWriteHandle, CharacterWriteGuard,
        ZoneLockEnforcer, ZoneTableWriteHandle,
    },
    mount::MountConfig,
    unique_guid::{
        npc_guid, player_guid, shorten_player_guid, zone_template_guid, FIXTURE_DISCRIMINANT,
    },
    update_position::UpdatePositionPacket,
    WriteLockingBroadcastSupplier,
};

const fn default_true() -> bool {
    true
}

const fn default_chunk_size() -> u16 {
    200
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointOfInterestConfig {
    pub guid: u32,
    pub pos: Pos,
    pub rot: Pos,
    #[serde(default)]
    pub name_id: u32,
    #[serde(default = "default_true")]
    pub teleport_enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneConfig {
    guid: u8,
    max_players: u32,
    template_name: u32,
    #[serde(default)]
    template_icon: u32,
    asset_name: String,
    hide_ui: bool,
    is_combat: bool,
    #[serde(default = "default_chunk_size")]
    chunk_size: u16,
    default_point_of_interest: PointOfInterestConfig,
    #[serde(default)]
    other_points_of_interest: Vec<PointOfInterestConfig>,
    #[serde(default)]
    spawn_sky: String,
    speed: f32,
    jump_height_multiplier: f32,
    gravity_multiplier: f32,
    doors: Vec<DoorConfig>,
    transports: Vec<TransportConfig>,
    ambient_npcs: Vec<AmbientNpcConfig>,
    seconds_per_day: u32,
    #[serde(default = "default_true")]
    update_previous_location_on_leave: bool,
    #[serde(default)]
    map_id: u32,
}

#[derive(Clone)]
pub struct ZoneTemplate {
    guid: u8,
    pub template_name: u32,
    pub template_icon: u32,
    pub max_players: u32,
    pub asset_name: String,
    pub chunk_size: u16,
    pub default_spawn_pos: Pos,
    pub default_spawn_rot: Pos,
    default_spawn_sky: String,
    pub speed: f32,
    pub jump_height_multiplier: f32,
    pub gravity_multiplier: f32,
    hide_ui: bool,
    is_combat: bool,
    characters: Vec<NpcTemplate>,
    pub seconds_per_day: u32,
    update_previous_location_on_leave: bool,
    map_id: u32,
}

impl Guid<u8> for ZoneTemplate {
    fn guid(&self) -> u8 {
        self.guid
    }
}

impl From<&Vec<Character>>
    for GuidTable<
        u64,
        Character,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
        CharacterSynchronizationIndex,
    >
{
    fn from(value: &Vec<Character>) -> Self {
        let table = GuidTable::new();

        {
            let mut write_handle = table.write();
            for character in value.iter() {
                if write_handle.insert(character.clone()).is_some() {
                    panic!("Two characters have same GUID {}", character.guid());
                }
            }
        }

        table
    }
}

impl From<ZoneConfig> for ZoneTemplate {
    fn from(value: ZoneConfig) -> Self {
        let mut characters = Vec::new();

        let mut index = 0;

        {
            for ambient_npc in value.ambient_npcs {
                characters.push(NpcTemplate::from_config(ambient_npc, index));
                index += 1;
            }

            for door in value.doors {
                characters.push(NpcTemplate::from_config(door, index));
                index += 1;
            }

            for transport in value.transports {
                characters.push(NpcTemplate::from_config(transport, index));
                index += 1;
            }
        }

        ZoneTemplate {
            guid: value.guid,
            template_name: value.template_name,
            max_players: value.max_players,
            template_icon: value.template_icon,
            asset_name: value.asset_name.clone(),
            chunk_size: value.chunk_size,
            default_spawn_pos: value.default_point_of_interest.pos,
            default_spawn_rot: value.default_point_of_interest.rot,
            default_spawn_sky: value.spawn_sky.clone(),
            speed: value.speed,
            jump_height_multiplier: value.jump_height_multiplier,
            gravity_multiplier: value.gravity_multiplier,
            hide_ui: value.hide_ui,
            is_combat: value.is_combat,
            characters,
            seconds_per_day: value.seconds_per_day,
            update_previous_location_on_leave: value.update_previous_location_on_leave,
            map_id: value.map_id,
        }
    }
}

impl ZoneTemplate {
    pub fn to_zone_instance(
        &self,
        instance_guid: u64,
        house_data: Option<House>,
        global_characters_table: &mut GuidTableWriteHandle<
            u64,
            Character,
            CharacterLocationIndex,
            CharacterNameIndex,
            CharacterSquadIndex,
            CharacterMatchmakingGroupIndex,
            CharacterSynchronizationIndex,
        >,
    ) -> ZoneInstance {
        let keys_to_guid: HashMap<&String, u64> = self
            .characters
            .iter()
            .filter_map(|template| {
                template
                    .key
                    .as_ref()
                    .map(|key| (key, template.guid(instance_guid)))
            })
            .collect();

        for character_template in self.characters.iter() {
            let character =
                character_template.to_character(instance_guid, self.chunk_size, &keys_to_guid);
            global_characters_table.insert(character);
        }

        ZoneInstance {
            guid: instance_guid,
            template_guid: Guid::guid(self),
            template_name: self.template_name,
            max_players: self.max_players,
            icon: self.template_icon,
            asset_name: self.asset_name.clone(),
            chunk_size: self.chunk_size,
            default_spawn_pos: self.default_spawn_pos,
            default_spawn_rot: self.default_spawn_rot,
            default_spawn_sky: self.default_spawn_sky.clone(),
            speed: self.speed,
            jump_height_multiplier: self.jump_height_multiplier,
            gravity_multiplier: self.gravity_multiplier,
            hide_ui: self.hide_ui,
            is_combat: self.is_combat,
            house_data,
            seconds_per_day: self.seconds_per_day,
            update_previous_location_on_leave: self.update_previous_location_on_leave,
            map_id: self.map_id,
        }
    }
}

pub struct House {
    pub owner: u32,
    pub owner_name: String,
    pub custom_name: String,
    pub rating: f32,
    pub total_votes: u32,
    pub fixtures: Vec<PreviousFixture>,
    pub build_areas: Vec<BuildArea>,
    pub is_locked: bool,
    pub is_published: bool,
    pub is_rateable: bool,
}

#[derive(Default)]
pub struct CharacterDiffResult {
    pub character_diffs_for_moved_character: BTreeMap<u64, bool>,
    pub players_too_far_from_moved_character: Vec<u32>,
    pub new_players_close_to_moved_character: Vec<u32>,
}

enum CharacterMovementType {
    SameChunk {
        chunk: Chunk,
        npcs_to_interact_with: Vec<u64>,
    },
    DifferentChunk {
        new_chunk: Chunk,
    },
}

pub struct ZoneInstance {
    guid: u64,
    pub template_guid: u8,
    pub template_name: u32,
    pub max_players: u32,
    pub icon: u32,
    pub asset_name: String,
    chunk_size: u16,
    pub default_spawn_pos: Pos,
    pub default_spawn_rot: Pos,
    default_spawn_sky: String,
    pub speed: f32,
    pub jump_height_multiplier: f32,
    pub gravity_multiplier: f32,
    hide_ui: bool,
    pub is_combat: bool,
    pub house_data: Option<House>,
    pub seconds_per_day: u32,
    update_previous_location_on_leave: bool,
    map_id: u32,
}

impl IndexedGuid<u64, u8> for ZoneInstance {
    fn guid(&self) -> u64 {
        self.guid
    }

    fn index1(&self) -> u8 {
        self.template_guid
    }
}

#[macro_export]
macro_rules! diff_character_handles {
    ($instance_guid:expr, $old_chunk:expr, $new_chunk:expr, $characters_table_write_handle:expr, $moved_character_guid:expr) => {{
        let character_diffs =
            $crate::game_server::handlers::zone::ZoneInstance::diff_character_guids(
                $instance_guid,
                $old_chunk,
                $new_chunk,
                $characters_table_write_handle,
                $moved_character_guid,
            );

        let mut handles = BTreeMap::new();
        for guid in character_diffs.character_diffs_for_moved_character.keys() {
            if let Some(character) = $characters_table_write_handle.get(*guid) {
                handles.insert(*guid, character.read());
            }
        }

        (character_diffs, handles)
    }};
}

impl ZoneInstance {
    pub fn new_house(
        guid: u64,
        template: &ZoneTemplate,
        house: House,
        global_characters_table: &mut GuidTableWriteHandle<
            u64,
            Character,
            CharacterLocationIndex,
            CharacterNameIndex,
            CharacterSquadIndex,
            CharacterMatchmakingGroupIndex,
            CharacterSynchronizationIndex,
        >,
    ) -> Self {
        for (index, fixture) in house.fixtures.iter().enumerate() {
            global_characters_table.insert(Character::new(
                npc_guid(FIXTURE_DISCRIMINANT, guid, index as u16),
                fixture.model_id,
                fixture.pos,
                fixture.rot,
                template.chunk_size,
                fixture.scale,
                CharacterType::Fixture(guid, fixture.as_current_fixture()),
                None,
                None,
                0.0,
                0.0,
                0.0,
                guid,
                WieldType::None,
                0,
                HashMap::new(),
                Vec::new(),
                None,
            ));
        }
        template.to_zone_instance(guid, Some(house), global_characters_table)
    }

    pub fn send_self(&self, sender: u32) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: ZoneDetails {
                name: self.asset_name.clone(),
                zone_type: 2,
                hide_ui: self.hide_ui,
                combat_hud: self.is_combat,
                sky_definition_file_name: self.default_spawn_sky.clone(),
                combat_camera: self.is_combat,
                unknown7: 0,
                unknown8: 0,
            },
        })];

        if let Some(house) = &self.house_data {
            packets.append(&mut prepare_init_house_packets(sender, self, house)?);
        }

        Ok(packets)
    }

    pub fn send_self_on_client_ready(&self) -> Vec<Vec<u8>> {
        vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: ExecuteScriptWithIntParams {
                    script_name: "UIGlobal.AtlasLoadZone".to_string(),
                    params: vec![self.map_id as i32],
                },
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: ExecuteScriptWithStringParams {
                    script_name: format!(
                        "CombatHandler.{}",
                        if self.is_combat { "show" } else { "hide" }
                    ),
                    params: vec![],
                },
            }),
        ]
    }

    fn nearby_chunks(center: Chunk) -> BTreeSet<Chunk> {
        let Chunk {
            x: center_x,
            z: center_z,
            size,
        } = center;
        BTreeSet::from_iter(vec![
            Chunk {
                x: center_x.saturating_sub(1),
                z: center_z.saturating_sub(1),
                size,
            },
            Chunk {
                x: center_x.saturating_sub(1),
                z: center_z,
                size,
            },
            Chunk {
                x: center_x.saturating_sub(1),
                z: center_z.saturating_add(1),
                size,
            },
            Chunk {
                x: center_x,
                z: center_z.saturating_sub(1),
                size,
            },
            Chunk {
                x: center_x,
                z: center_z,
                size,
            },
            Chunk {
                x: center_x,
                z: center_z.saturating_add(1),
                size,
            },
            Chunk {
                x: center_x.saturating_add(1),
                z: center_z.saturating_sub(1),
                size,
            },
            Chunk {
                x: center_x.saturating_add(1),
                z: center_z,
                size,
            },
            Chunk {
                x: center_x.saturating_add(1),
                z: center_z.saturating_add(1),
                size,
            },
        ])
    }

    pub fn other_players_nearby<'a>(
        sender: Option<u32>,
        chunk: Chunk,
        instance_guid: u64,
        characters_table_handle: &'a impl GuidTableIndexer<
            'a,
            u64,
            Character,
            CharacterLocationIndex,
            CharacterNameIndex,
            CharacterSquadIndex,
            CharacterMatchmakingGroupIndex,
            CharacterSynchronizationIndex,
        >,
    ) -> Vec<u32> {
        let mut guids = Vec::new();

        for chunk in ZoneInstance::nearby_chunks(chunk) {
            for guid in characters_table_handle.keys_by_index1((
                CharacterCategory::PlayerReady,
                instance_guid,
                chunk,
            )) {
                if sender
                    .map(|sender_guid| guid != player_guid(sender_guid))
                    .unwrap_or(true)
                {
                    match shorten_player_guid(guid) {
                        Ok(short_guid) => guids.push(short_guid),
                        Err(err) => info!(
                            "Skipped nearby player {} because their GUID is not a player's GUID: {}",
                            guid,
                            err
                        ),
                    }
                }
            }
        }

        guids
    }

    pub fn all_players_nearby<'a>(
        chunk: Chunk,
        instance_guid: u64,
        characters_table_handle: &'a impl GuidTableIndexer<
            'a,
            u64,
            Character,
            CharacterLocationIndex,
            CharacterNameIndex,
            CharacterSquadIndex,
            CharacterMatchmakingGroupIndex,
            CharacterSynchronizationIndex,
        >,
    ) -> Vec<u32> {
        ZoneInstance::other_players_nearby(None, chunk, instance_guid, characters_table_handle)
    }

    pub fn diff_character_guids<'a>(
        instance_guid: u64,
        old_chunk: Chunk,
        new_chunk: Chunk,
        characters_table_handle: &'a impl GuidTableIndexer<
            'a,
            u64,
            Character,
            CharacterLocationIndex,
            CharacterNameIndex,
            CharacterSquadIndex,
            CharacterMatchmakingGroupIndex,
            CharacterSynchronizationIndex,
        >,
        moved_character_guid: u64,
    ) -> CharacterDiffResult {
        let old_chunks = ZoneInstance::nearby_chunks(old_chunk);
        let new_chunks = ZoneInstance::nearby_chunks(new_chunk);
        let chunks_to_remove: Vec<&Chunk> = old_chunks.difference(&new_chunks).collect();
        let chunks_to_add: Vec<&Chunk> = new_chunks.difference(&old_chunks).collect();

        let mut character_diffs_for_moved_character = BTreeMap::new();
        let mut players_too_far_from_moved_character = Vec::new();
        let mut new_players_close_to_moved_character = Vec::new();
        for category in all::<CharacterCategory>() {
            for chunk in chunks_to_remove.iter() {
                for guid in
                    characters_table_handle.keys_by_index1((category, instance_guid, **chunk))
                {
                    character_diffs_for_moved_character.insert(guid, false);

                    if category == CharacterCategory::PlayerReady && guid != moved_character_guid {
                        if let Ok(player_guid) = shorten_player_guid(guid) {
                            players_too_far_from_moved_character.push(player_guid);
                        }
                    }
                }
            }
        }

        for category in all::<CharacterCategory>() {
            for chunk in chunks_to_add.iter() {
                for guid in
                    characters_table_handle.keys_by_index1((category, instance_guid, **chunk))
                {
                    character_diffs_for_moved_character.insert(guid, true);

                    if category == CharacterCategory::PlayerReady && guid != moved_character_guid {
                        if let Ok(player_guid) = shorten_player_guid(guid) {
                            new_players_close_to_moved_character.push(player_guid);
                        }
                    }
                }
            }
        }

        character_diffs_for_moved_character.remove(&moved_character_guid);

        CharacterDiffResult {
            character_diffs_for_moved_character,
            players_too_far_from_moved_character,
            new_players_close_to_moved_character,
        }
    }

    pub fn diff_character_broadcasts(
        moved_character_guid: u64,
        character_diffs: CharacterDiffResult,
        characters_read: &BTreeMap<u64, CharacterReadGuard<'_>>,
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> Vec<Broadcast> {
        let mut broadcasts = Vec::new();

        if let Ok(moved_player_guid) = shorten_player_guid(moved_character_guid) {
            let mut diff_packets: Vec<Vec<u8>> = Vec::new();

            for (guid, add) in &character_diffs.character_diffs_for_moved_character {
                if let Some(character) = characters_read.get(guid) {
                    if *add {
                        diff_packets.append(&mut character.stats.add_packets(
                            false,
                            mount_configs,
                            item_definitions,
                            customizations,
                        ));
                    } else {
                        diff_packets
                            .append(&mut character.stats.remove_packets(RemovalMode::default()));
                    }
                }
            }

            broadcasts.push(Broadcast::Single(moved_player_guid, diff_packets));
        }

        if let Some(moved_character_read_handle) = characters_read.get(&moved_character_guid) {
            broadcasts.push(Broadcast::Multi(
                character_diffs.new_players_close_to_moved_character,
                moved_character_read_handle.stats.add_packets(
                    false,
                    mount_configs,
                    item_definitions,
                    customizations,
                ),
            ));
            broadcasts.push(Broadcast::Multi(
                character_diffs.players_too_far_from_moved_character,
                moved_character_read_handle
                    .stats
                    .remove_packets(RemovalMode::default()),
            ));
        }

        broadcasts
    }

    fn move_character_with_locks(
        auto_interact_npcs: &[u64],
        characters_read: BTreeMap<u64, CharacterReadGuard<'_>>,
        moved_character_write_handle: &mut CharacterWriteGuard<'_>,
        new_pos: Pos,
        new_rot: Pos,
    ) -> Vec<u64> {
        let previous_pos = moved_character_write_handle.stats.pos;
        moved_character_write_handle.stats.pos = new_pos;
        moved_character_write_handle.stats.rot = new_rot;

        let mut characters_to_interact = Vec::new();
        for npc_guid in auto_interact_npcs {
            if let Some(npc_read_handle) = characters_read.get(npc_guid) {
                if npc_read_handle.stats.auto_interact_radius > 0.0 {
                    let distance_now = distance3(
                        moved_character_write_handle.stats.pos.x,
                        moved_character_write_handle.stats.pos.y,
                        moved_character_write_handle.stats.pos.z,
                        npc_read_handle.stats.pos.x,
                        npc_read_handle.stats.pos.y,
                        npc_read_handle.stats.pos.z,
                    );
                    let distance_before = distance3(
                        previous_pos.x,
                        previous_pos.y,
                        previous_pos.z,
                        npc_read_handle.stats.pos.x,
                        npc_read_handle.stats.pos.y,
                        npc_read_handle.stats.pos.z,
                    );

                    // Only trigger the interaction when the player first enters the radius
                    if distance_now <= npc_read_handle.stats.auto_interact_radius
                        && distance_before > npc_read_handle.stats.auto_interact_radius
                    {
                        characters_to_interact.push(npc_read_handle.guid());
                    }
                }
            }
        }

        characters_to_interact
    }

    pub fn move_character<T: UpdatePositionPacket>(
        mut full_update_packet: T,
        should_teleport: bool,
        game_server: &GameServer,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let moved_character_guid = full_update_packet.guid();
        let new_pos = full_update_packet.pos();
        let new_rot = full_update_packet.rot();

        let movement_result: Result<(Vec<Broadcast>, CharacterMovementType), ProcessPacketError> =
            game_server
                .lock_enforcer()
                .read_characters(|characters_table_read_handle| {
                    let chunk_test_result = characters_table_read_handle
                        .index1(moved_character_guid)
                        .ok_or_else(|| ProcessPacketError::new_with_log_level(ProcessPacketErrorType::ConstraintViolated, format!("Tried to move character {moved_character_guid} who does not exist"), LogLevel::Debug))
                        .map(|(_, instance_guid, old_chunk)| {
                            let new_chunk = Character::chunk(new_pos.x, new_pos.z, old_chunk.size);

                            let same_chunk = old_chunk == new_chunk;
                            if same_chunk {
                                let auto_interactable_npcs: Vec<u64> = characters_table_read_handle
                                    .keys_by_index1((
                                        CharacterCategory::NpcAutoInteractable,
                                        instance_guid,
                                        new_chunk,
                                    ))
                                    .chain(characters_table_read_handle.keys_by_index1((
                                        CharacterCategory::NpcAutoInteractableTickable(
                                            TickableNpcSynchronization::Synchronized,
                                        ),
                                        instance_guid,
                                        new_chunk,
                                    )))
                                    .chain(characters_table_read_handle.keys_by_index1((
                                        CharacterCategory::NpcAutoInteractableTickable(
                                            TickableNpcSynchronization::Unsynchronized,
                                        ),
                                        instance_guid,
                                        new_chunk,
                                    )))
                                    .collect();

                                (
                                    instance_guid,
                                    CharacterMovementType::SameChunk {
                                        chunk: new_chunk,
                                        npcs_to_interact_with: auto_interactable_npcs,
                                    },
                                )
                            } else {
                                (instance_guid, CharacterMovementType::DifferentChunk { new_chunk })
                            }
                        });

                    let (read_guids, write_guids) = match &chunk_test_result {
                        Ok((_, movement_type)) => match movement_type {
                            CharacterMovementType::SameChunk {
                                npcs_to_interact_with,
                                ..
                            } => (npcs_to_interact_with.clone(), vec![moved_character_guid]),
                            CharacterMovementType::DifferentChunk { .. } => (Vec::new(), Vec::new()),
                        },
                        Err(_) => (Vec::new(), Vec::new()),
                    };

                    CharacterLockRequest {
                        read_guids,
                        write_guids,
                        character_consumer: move |characters_table_read_handle,
                                                  characters_read,
                                                  mut characters_write,
                                                  _| {
                            let (instance_guid, movement_type) = chunk_test_result?;

                            let CharacterMovementType::SameChunk {
                                chunk,
                                npcs_to_interact_with,
                            } = movement_type
                            else {
                                return Ok((Vec::new(), movement_type));
                            };

                            let mut broadcasts = Vec::new();
                            let jump_multiplier = characters_write
                                .get(&moved_character_guid)
                                .map(|character_handle| {
                                    character_handle.stats.jump_height_multiplier.total()
                                })
                                .unwrap_or(1.0);
                            full_update_packet.apply_jump_height_multiplier(jump_multiplier);

                            let filtered_npcs_to_interact_with = ZoneInstance::move_character_with_locks(
                                &npcs_to_interact_with,
                                characters_read,
                                characters_write.get_mut(&moved_character_guid).unwrap(),
                                new_pos,
                                new_rot,
                            );

                            // We don't return this value when the chunks are different, as players could change between when
                            // we release the read lock and acquire the write lock
                            let other_players_nearby = ZoneInstance::other_players_nearby(
                                shorten_player_guid(moved_character_guid).ok(),
                                chunk,
                                instance_guid,
                                characters_table_read_handle,
                            );
                            broadcasts.push(Broadcast::Multi(
                                other_players_nearby,
                                vec![GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: full_update_packet,
                                })],
                            ));
                            Ok((
                                broadcasts,
                                CharacterMovementType::SameChunk {
                                    chunk,
                                    npcs_to_interact_with: filtered_npcs_to_interact_with,
                                },
                            ))
                        },
                    }
                });

        let (mut broadcasts, movement_type) = movement_result?;

        let npcs_to_interact_with = match movement_type {
            CharacterMovementType::SameChunk {
                npcs_to_interact_with,
                ..
            } => npcs_to_interact_with,
            CharacterMovementType::DifferentChunk { new_chunk } => {
                game_server
                    .lock_enforcer()
                    .write_characters(|characters_table_write_handle, _| {
                        characters_table_write_handle.update_value_indices(
                            moved_character_guid,
                            |possible_character_write_handle, characters_table_write_handle| {
                                let Some(moved_character_write_handle) =
                                    possible_character_write_handle
                                else {
                                    return Vec::new();
                                };

                                let (_, instance_guid, old_chunk) =
                                    moved_character_write_handle.index1();

                                let (character_diffs, mut characters_read) = diff_character_handles!(
                                    instance_guid,
                                    old_chunk,
                                    new_chunk,
                                    characters_table_write_handle,
                                    moved_character_guid
                                );

                                let auto_interactable_npcs: Vec<u64> = characters_table_write_handle
                                    .keys_by_index1((
                                        CharacterCategory::NpcAutoInteractable,
                                        instance_guid,
                                        new_chunk,
                                    ))
                                    .chain(characters_table_write_handle.keys_by_index1((
                                        CharacterCategory::NpcAutoInteractableTickable(
                                            TickableNpcSynchronization::Synchronized,
                                        ),
                                        instance_guid,
                                        new_chunk,
                                    )))
                                    .chain(characters_table_write_handle.keys_by_index1((
                                        CharacterCategory::NpcAutoInteractableTickable(
                                            TickableNpcSynchronization::Unsynchronized,
                                        ),
                                        instance_guid,
                                        new_chunk,
                                    )))
                                    .collect();
                                for npc_guid in auto_interactable_npcs.iter() {
                                    if let Some(npc) = characters_table_write_handle.get(*npc_guid) {
                                        characters_read.insert(*npc_guid, npc.read());
                                    }
                                }

                                broadcasts.append(&mut ZoneInstance::diff_character_broadcasts(
                                    moved_character_guid,
                                    character_diffs,
                                    &characters_read,
                                    game_server.mounts(),
                                    game_server.items(),
                                    game_server.customizations(),
                                ));

                                // Remove the moved character when they change chunks
                                let previous_other_players_nearby = ZoneInstance::other_players_nearby(
                                    shorten_player_guid(moved_character_guid).ok(),
                                    old_chunk,
                                    instance_guid,
                                    characters_table_write_handle,
                                );
                                broadcasts.push(Broadcast::Multi(
                                    previous_other_players_nearby,
                                    moved_character_write_handle
                                        .stats
                                        .remove_packets(RemovalMode::default()),
                                ));

                                // Move the character
                                let characters_to_interact = ZoneInstance::move_character_with_locks(
                                    &auto_interactable_npcs,
                                    characters_read,
                                    moved_character_write_handle,
                                    new_pos,
                                    new_rot,
                                );

                                let jump_multiplier = moved_character_write_handle
                                    .stats
                                    .jump_height_multiplier
                                    .total();
                                full_update_packet.apply_jump_height_multiplier(jump_multiplier);

                                let other_players_nearby = ZoneInstance::other_players_nearby(
                                    shorten_player_guid(moved_character_guid).ok(),
                                    new_chunk,
                                    instance_guid,
                                    characters_table_write_handle,
                                );
                                let mut new_chunk_packets =
                                    moved_character_write_handle.stats.add_packets(
                                        false,
                                        game_server.mounts(),
                                        game_server.items(),
                                        game_server.customizations(),
                                    );
                                new_chunk_packets.push(GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: full_update_packet,
                                }));
                                broadcasts
                                    .push(Broadcast::Multi(other_players_nearby, new_chunk_packets));

                                characters_to_interact
                            },
                        )
                    })
            }
        };

        for character_guid in npcs_to_interact_with {
            broadcasts.append(&mut interact_with_character(
                moved_character_guid,
                character_guid,
                game_server,
            )?);
        }

        if should_teleport {
            if let Ok(moved_player_guid) = shorten_player_guid(moved_character_guid) {
                broadcasts.push(Broadcast::Single(
                    moved_player_guid,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: Position {
                            player_pos: new_pos,
                            rot: new_rot,
                            is_teleport: true,
                            unknown2: true,
                        },
                    })],
                ));
            }
        }

        Ok(broadcasts)
    }
}

type ZoneTemplateMap = BTreeMap<u8, ZoneTemplate>;
type PointOfInterestMap = BTreeMap<u32, (u8, PointOfInterestConfig)>;
type LoadedZones = (
    ZoneTemplateMap,
    GuidTable<u64, ZoneInstance, u8>,
    PointOfInterestMap,
);
pub fn load_zones(config_dir: &Path) -> Result<LoadedZones, ConfigError> {
    let mut file = File::open(config_dir.join("zones.yaml"))?;
    let zone_configs: Vec<ZoneConfig> = serde_yaml::from_reader(&mut file)?;

    let mut templates = BTreeMap::new();
    let zones = GuidTable::new();
    let mut points_of_interest = BTreeMap::new();
    {
        for zone_config in zone_configs {
            let iter = zone_config
                .other_points_of_interest
                .iter()
                .chain(iter::once(&zone_config.default_point_of_interest));
            for point_of_interest in iter {
                if points_of_interest
                    .insert(
                        point_of_interest.guid,
                        (zone_config.guid, point_of_interest.clone()),
                    )
                    .is_some()
                {
                    panic!("Two points of interest have ID {}", point_of_interest.guid);
                }
            }

            let template: ZoneTemplate = zone_config.into();
            let template_guid = Guid::guid(&template);

            if template.chunk_size == 0 {
                panic!("Zone template {template_guid} cannot have a chunk size of 0");
            }

            if templates.insert(template_guid, template).is_some() {
                panic!("Two zone templates have ID {template_guid}");
            }
        }
    }

    Ok((templates, zones, points_of_interest))
}

pub fn enter_zone(
    characters_table_write_handle: &mut CharacterTableWriteHandle,
    player: u32,
    destination_read_handle: &RwLockReadGuard<ZoneInstance>,
    destination_pos: Option<Pos>,
    destination_rot: Option<Pos>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let destination_pos = destination_pos.unwrap_or(destination_read_handle.default_spawn_pos);
    let destination_rot = destination_rot.unwrap_or(destination_read_handle.default_spawn_rot);

    // Perform fallible operations before we update player data to avoid an inconsistent state
    let mut broadcasts = prepare_init_zone_packets(
        player,
        destination_read_handle,
        destination_pos,
        destination_rot,
    )?;

    characters_table_write_handle.update_value_indices(
        player_guid(player),
        |possible_character_write_handle, characters_table_write_handle| {
            if let Some(character_write_handle) = possible_character_write_handle {
                let (_, instance_guid, chunk) = character_write_handle.index1();
                let other_players_nearby = ZoneInstance::other_players_nearby(
                    Some(player),
                    chunk,
                    instance_guid,
                    characters_table_write_handle,
                );
                broadcasts.push(Broadcast::Multi(
                    other_players_nearby,
                    character_write_handle
                        .stats
                        .remove_packets(RemovalMode::default()),
                ));

                let previous_zone_template_guid =
                    zone_template_guid(character_write_handle.stats.instance_guid);
                let previous_pos = character_write_handle.stats.pos;
                let previous_rot = character_write_handle.stats.rot;

                if let CharacterType::Player(ref mut player) =
                    &mut character_write_handle.stats.character_type
                {
                    player.ready = false;

                    if player.update_previous_location_on_leave {
                        player.previous_location = PreviousLocation {
                            template_guid: previous_zone_template_guid,
                            pos: previous_pos,
                            rot: previous_rot,
                        }
                    }
                    player.update_previous_location_on_leave =
                        destination_read_handle.update_previous_location_on_leave;
                }
                character_write_handle.stats.instance_guid = destination_read_handle.guid;
                character_write_handle.stats.pos = destination_pos;
                character_write_handle.stats.rot = destination_rot;
                character_write_handle.stats.chunk_size = destination_read_handle.chunk_size;
            }
        },
    );

    Ok(broadcasts)
}

fn prepare_init_zone_packets(
    player: u32,
    destination: &RwLockReadGuard<ZoneInstance>,
    destination_pos: Pos,
    destination_rot: Pos,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let zone_name = destination.asset_name.clone();
    let mut packets = vec![];
    packets.push(GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: ClientBeginZoning {
            zone_name,
            zone_type: 2,
            pos: destination_pos,
            rot: destination_rot,
            sky_definition_file_name: destination.default_spawn_sky.clone(),
            unknown1: false,
            zone_id: 0,
            zone_name_id: 0,
            world_id: 0,
            world_name_id: 0,
            unknown6: false,
            unknown7: false,
        },
    }));

    packets.append(&mut destination.send_self(player)?);

    Ok(vec![Broadcast::Single(player, packets)])
}

pub fn clean_up_zone_if_no_players(
    instance_guid: u64,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
) {
    let ready_range = (
        CharacterCategory::PlayerReady,
        instance_guid,
        Character::MIN_CHUNK,
    )
        ..=(
            CharacterCategory::PlayerReady,
            instance_guid,
            Character::MAX_CHUNK,
        );
    let unready_range = (
        CharacterCategory::PlayerUnready,
        instance_guid,
        Character::MIN_CHUNK,
    )
        ..=(
            CharacterCategory::PlayerUnready,
            instance_guid,
            Character::MAX_CHUNK,
        );

    let has_ready_players = characters_table_write_handle.any_by_index1_range(ready_range);
    let has_unready_players = characters_table_write_handle.any_by_index1_range(unready_range);

    if !has_ready_players && !has_unready_players {
        clean_up_zone(
            instance_guid,
            characters_table_write_handle,
            zones_table_write_handle,
        );
    }
}

fn clean_up_zone(
    instance_guid: u64,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
) {
    for category in all::<CharacterCategory>() {
        let range = (category, instance_guid, Character::MIN_CHUNK)
            ..=(category, instance_guid, Character::MAX_CHUNK);
        let characters_to_remove: Vec<u64> = characters_table_write_handle
            .keys_by_index1_range(range)
            .collect();
        for character_guid in characters_to_remove {
            characters_table_write_handle.remove(character_guid);
        }
    }

    zones_table_write_handle.remove(instance_guid);
}

#[macro_export]
macro_rules! teleport_to_zone {
    ($characters_table_write_handle:expr, $player:expr, $zones_table_write_handle:expr,
     $destination_read_handle:expr, $destination_pos:expr, $destination_rot:expr, $mounts:expr$(,)?) => {{
        let character = $crate::game_server::handlers::guid::GuidTableHandle::get(
            $characters_table_write_handle,
            player_guid($player),
        );

        let mut broadcasts = Vec::new();
        let mut possible_previous_instance_guid = None;
        if let Some(character_lock) = character {
            broadcasts.append(&mut $crate::game_server::handlers::mount::reply_dismount(
                $player,
                $characters_table_write_handle,
                $destination_read_handle,
                &mut character_lock.write(),
                $mounts,
            )?);
            possible_previous_instance_guid = Some(character_lock.read().stats.instance_guid);
        }

        broadcasts.append(&mut $crate::game_server::handlers::zone::enter_zone(
            $characters_table_write_handle,
            $player,
            $destination_read_handle,
            $destination_pos,
            $destination_rot,
        )?);

        if let Some(previous_instance_guid) = possible_previous_instance_guid {
            $crate::game_server::handlers::zone::clean_up_zone_if_no_players(
                previous_instance_guid,
                $characters_table_write_handle,
                $zones_table_write_handle,
            );
        }

        Ok(broadcasts)
    }};
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DestinationZoneInstance {
    #[default]
    Same,
    Other {
        instance_guid: u64,
    },
    Any {
        template_guid: u8,
    },
}

pub fn teleport_anywhere(
    destination_pos: Pos,
    destination_rot: Pos,
    destination_zone: DestinationZoneInstance,
    requester: u32,
) -> WriteLockingBroadcastSupplier {
    coerce_to_broadcast_supplier(move |game_server| {
        game_server.lock_enforcer().write_characters(
            |characters_table_write_handle, minigame_data_lock_enforcer| {
                let zones_lock_enforcer: ZoneLockEnforcer<'_> = minigame_data_lock_enforcer.into();
                zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                    let source_zone_guid =
                        match characters_table_write_handle.get(player_guid(requester)) {
                            Some(character) => Ok(character.read().stats.instance_guid),
                            None => Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!("Tried to teleport unknown player {requester} anywhere"),
                            )),
                        }?;

                    let destination_zone_guid = match destination_zone {
                        DestinationZoneInstance::Same => source_zone_guid,
                        DestinationZoneInstance::Other { instance_guid } => instance_guid,
                        DestinationZoneInstance::Any { template_guid } => game_server
                            .get_or_create_instance(
                                characters_table_write_handle,
                                zones_table_write_handle,
                                template_guid,
                                1,
                            )?,
                    };

                    if source_zone_guid != destination_zone_guid {
                        let Some(destination_lock) =
                            zones_table_write_handle.get(destination_zone_guid)
                        else {
                            return Ok(Vec::new());
                        };

                        teleport_to_zone!(
                            characters_table_write_handle,
                            requester,
                            zones_table_write_handle,
                            &destination_lock.read(),
                            Some(destination_pos),
                            Some(destination_rot),
                            game_server.mounts(),
                        )
                    } else {
                        Ok(teleport_within_zone(
                            requester,
                            destination_pos,
                            destination_rot,
                        ))
                    }
                })
            },
        )
    })
}

pub fn interact_with_character(
    requester: u64,
    target: u64,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let requester_guid = shorten_player_guid(requester)?;
    let broadcast_supplier: WriteLockingBroadcastSupplier =
        game_server.lock_enforcer().read_characters(|_| {
            CharacterLockRequest {
                read_guids: Vec::new(),
                write_guids: vec![requester, target],
                character_consumer: move |_, _, mut characters_write, _| {
                    let Some(requester_read_handle) = characters_write.get(&requester) else {
                        return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                    };

                    let source_zone_guid = requester_read_handle.stats.instance_guid;
                    let requester_x = requester_read_handle.stats.pos.x;
                    let requester_y = requester_read_handle.stats.pos.y;
                    let requester_z = requester_read_handle.stats.pos.z;

                    let Some(target_read_handle) = characters_write.get_mut(&target) else {
                        return Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Received request to interact with unknown NPC {target} from {requester}"
                            ),
                        ));
                    };

                    // Ensure the character is close enough to interact
                    // Otherwise send MoveToInteract
                    let distance = distance3(
                        requester_x,
                        requester_y,
                        requester_z,
                        target_read_handle.stats.pos.x,
                        target_read_handle.stats.pos.y,
                        target_read_handle.stats.pos.z,
                    );
                    if distance > target_read_handle.stats.interact_radius
                        || target_read_handle.stats.instance_guid != source_zone_guid
                    {
                        let mut broadcasts = Vec::new();

                        let interaction_angle = (target_read_handle.stats.pos.z - requester_z)
                            .atan2(target_read_handle.stats.pos.x - requester_x);

                        let destination = Pos {
                            x: target_read_handle.stats.pos.x
                                - target_read_handle.stats.move_to_interact_offset
                                    * interaction_angle.cos(),
                            y: target_read_handle.stats.pos.y,
                            z: target_read_handle.stats.pos.z
                                - target_read_handle.stats.move_to_interact_offset
                                    * interaction_angle.sin(),
                            w: 1.0,
                        };

                        broadcasts.push(Broadcast::Single(
                            requester_guid,
                            vec![GamePacket::serialize(&TunneledPacket {
                                unknown1: true,
                                inner: MoveToInteract {
                                    destination,
                                    target,
                                },
                            })],
                        ));
                        return coerce_to_broadcast_supplier(|_| Ok(broadcasts));
                    }

                    target_read_handle.interact(requester_guid)
                },
            }
        });

    broadcast_supplier?(game_server)
}

pub fn teleport_within_zone(
    sender: u32,
    destination_pos: Pos,
    destination_rot: Pos,
) -> Vec<Broadcast> {
    vec![Broadcast::Single(
        sender,
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: Position {
                player_pos: destination_pos,
                rot: destination_rot,
                is_teleport: true,
                unknown2: true,
            },
        })],
    )]
}

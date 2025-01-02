use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    io::Error,
    path::Path,
};

use packet_serialize::SerializePacketError;
use parking_lot::RwLockReadGuard;
use serde::Deserialize;

use crate::{
    game_server::{
        packets::{
            client_update::Position,
            command::SelectPlayer,
            housing::BuildArea,
            item::{ItemDefinition, WieldType},
            login::{ClientBeginZoning, ZoneDetails},
            player_update::Customization,
            tunnel::TunneledPacket,
            ui::ExecuteScriptWithParams,
            update_position::UpdatePlayerPosition,
            GamePacket, Pos,
        },
        Broadcast, GameServer, ProcessPacketError,
    },
    info,
};

use super::{
    character::{
        coerce_to_broadcast_supplier, AmbientNpcConfig, Character, CharacterCategory,
        CharacterIndex, CharacterType, Chunk, DoorConfig, NpcTemplate, PreviousFixture,
        PreviousLocation, TransportConfig, WriteLockingBroadcastSupplier,
    },
    distance3,
    guid::{Guid, GuidTable, GuidTableIndexer, GuidTableWriteHandle, IndexedGuid},
    housing::prepare_init_house_packets,
    lock_enforcer::{
        CharacterLockRequest, CharacterReadGuard, CharacterTableWriteHandle, CharacterWriteGuard,
    },
    mount::MountConfig,
    unique_guid::{
        npc_guid, player_guid, shorten_player_guid, zone_template_guid, AMBIENT_NPC_DISCRIMINANT,
        FIXTURE_DISCRIMINANT,
    },
};

use strum::IntoEnumIterator;

#[derive(Deserialize)]
struct ZoneConfig {
    guid: u8,
    max_players: u32,
    template_name: u32,
    template_icon: Option<u32>,
    asset_name: String,
    hide_ui: bool,
    is_combat: bool,
    spawn_pos_x: f32,
    spawn_pos_y: f32,
    spawn_pos_z: f32,
    spawn_pos_w: f32,
    spawn_rot_x: f32,
    spawn_rot_y: f32,
    spawn_rot_z: f32,
    spawn_rot_w: f32,
    spawn_sky: Option<String>,
    speed: f32,
    jump_height_multiplier: f32,
    gravity_multiplier: f32,
    doors: Vec<DoorConfig>,
    interact_radius: f32,
    door_auto_interact_radius: f32,
    transports: Vec<TransportConfig>,
    ambient_npcs: Vec<AmbientNpcConfig>,
    seconds_per_day: u32,
}

#[derive(Clone)]
pub struct ZoneTemplate {
    guid: u8,
    pub template_name: u32,
    pub template_icon: u32,
    pub max_players: u32,
    pub asset_name: String,
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
}

impl Guid<u8> for ZoneTemplate {
    fn guid(&self) -> u8 {
        self.guid
    }
}

impl From<&Vec<Character>> for GuidTable<u64, Character, CharacterIndex> {
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

impl ZoneTemplate {
    pub fn to_zone_instance(
        &self,
        instance_guid: u64,
        house_data: Option<House>,
        global_characters_table: &mut GuidTableWriteHandle<u64, Character, CharacterIndex>,
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
            let character = character_template.to_character(instance_guid, &keys_to_guid);
            global_characters_table.insert(character);
        }

        ZoneInstance {
            guid: instance_guid,
            template_guid: Guid::guid(self),
            template_name: self.template_name,
            max_players: self.max_players,
            icon: self.template_icon,
            asset_name: self.asset_name.clone(),
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

pub struct ZoneInstance {
    guid: u64,
    pub template_guid: u8,
    pub template_name: u32,
    pub max_players: u32,
    pub icon: u32,
    pub asset_name: String,
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
}

impl IndexedGuid<u64, u8> for ZoneInstance {
    fn guid(&self) -> u64 {
        self.guid
    }

    fn index(&self) -> u8 {
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
        global_characters_table: &mut GuidTableWriteHandle<u64, Character, CharacterIndex>,
    ) -> Self {
        for (index, fixture) in house.fixtures.iter().enumerate() {
            global_characters_table.insert(Character::new(
                npc_guid(FIXTURE_DISCRIMINANT, guid, index as u16),
                fixture.pos,
                fixture.rot,
                fixture.scale,
                CharacterType::Fixture(guid, fixture.as_current_fixture()),
                None,
                None,
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

    pub fn send_self(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        Ok(vec![GamePacket::serialize(&TunneledPacket {
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
        })?])
    }

    fn nearby_chunks(center: Chunk) -> BTreeSet<Chunk> {
        let (center_x, center_z) = center;
        BTreeSet::from_iter(vec![
            (center_x.saturating_sub(1), center_z.saturating_sub(1)),
            (center_x.saturating_sub(1), center_z),
            (center_x.saturating_sub(1), center_z.saturating_add(1)),
            (center_x, center_z.saturating_sub(1)),
            (center_x, center_z),
            (center_x, center_z.saturating_add(1)),
            (center_x.saturating_add(1), center_z.saturating_sub(1)),
            (center_x.saturating_add(1), center_z),
            (center_x.saturating_add(1), center_z.saturating_add(1)),
        ])
    }

    pub fn other_players_nearby<'a>(
        sender: Option<u32>,
        chunk: Chunk,
        instance_guid: u64,
        characters_table_handle: &'a impl GuidTableIndexer<'a, u64, Character, CharacterIndex>,
    ) -> Result<Vec<u32>, ProcessPacketError> {
        let mut guids = Vec::new();

        for chunk in ZoneInstance::nearby_chunks(chunk) {
            for guid in characters_table_handle.keys_by_index((
                CharacterCategory::PlayerReady,
                instance_guid,
                chunk,
            )) {
                if sender
                    .map(|sender_guid| guid != player_guid(sender_guid))
                    .unwrap_or(true)
                {
                    guids.push(shorten_player_guid(guid)?);
                }
            }
        }

        Ok(guids)
    }

    pub fn all_players_nearby<'a>(
        sender: Option<u32>,
        chunk: Chunk,
        instance_guid: u64,
        characters_table_handle: &'a impl GuidTableIndexer<'a, u64, Character, CharacterIndex>,
    ) -> Result<Vec<u32>, ProcessPacketError> {
        let mut guids = ZoneInstance::other_players_nearby(
            sender,
            chunk,
            instance_guid,
            characters_table_handle,
        )?;
        if let Some(sender_guid) = sender {
            guids.push(sender_guid);
        }
        Ok(guids)
    }

    pub fn diff_character_guids<'a>(
        instance_guid: u64,
        old_chunk: Chunk,
        new_chunk: Chunk,
        characters_table_handle: &'a impl GuidTableIndexer<'a, u64, Character, CharacterIndex>,
        moved_character_guid: u64,
    ) -> CharacterDiffResult {
        let old_chunks = ZoneInstance::nearby_chunks(old_chunk);
        let new_chunks = ZoneInstance::nearby_chunks(new_chunk);
        let chunks_to_remove: Vec<&Chunk> = old_chunks.difference(&new_chunks).collect();
        let chunks_to_add: Vec<&Chunk> = new_chunks.difference(&old_chunks).collect();

        let mut character_diffs_for_moved_character = BTreeMap::new();
        let mut players_too_far_from_moved_character = Vec::new();
        let mut new_players_close_to_moved_character = Vec::new();
        for category in CharacterCategory::iter() {
            for chunk in chunks_to_remove.iter() {
                for guid in
                    characters_table_handle.keys_by_index((category, instance_guid, **chunk))
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

        for category in CharacterCategory::iter() {
            for chunk in chunks_to_add.iter() {
                for guid in
                    characters_table_handle.keys_by_index((category, instance_guid, **chunk))
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
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let mut broadcasts = Vec::new();

        if let Ok(moved_player_guid) = shorten_player_guid(moved_character_guid) {
            let mut diff_packets: Vec<Vec<u8>> = Vec::new();

            for (guid, add) in &character_diffs.character_diffs_for_moved_character {
                if let Some(character) = characters_read.get(guid) {
                    if *add {
                        diff_packets.append(&mut character.add_packets(
                            mount_configs,
                            item_definitions,
                            customizations,
                        )?);
                    } else {
                        diff_packets.append(&mut character.remove_packets()?);
                    }
                }
            }

            broadcasts.push(Broadcast::Single(moved_player_guid, diff_packets));
        }

        if let Some(moved_character_read_handle) = characters_read.get(&moved_character_guid) {
            broadcasts.push(Broadcast::Multi(
                character_diffs.new_players_close_to_moved_character,
                moved_character_read_handle.add_packets(
                    mount_configs,
                    item_definitions,
                    customizations,
                )?,
            ));
            broadcasts.push(Broadcast::Multi(
                character_diffs.players_too_far_from_moved_character,
                moved_character_read_handle.remove_packets()?,
            ));
        }

        Ok(broadcasts)
    }

    fn move_character_with_locks(
        auto_interact_npcs: Vec<u64>,
        characters_read: BTreeMap<u64, CharacterReadGuard<'_>>,
        mut characters_write: BTreeMap<u64, CharacterWriteGuard<'_>>,
        moved_character_guid: u64,
        new_pos: Pos,
        new_rot: Pos,
    ) -> Result<Vec<u64>, ProcessPacketError> {
        if let Some(character_write_handle) = characters_write.get_mut(&moved_character_guid) {
            let previous_pos = character_write_handle.stats.pos;
            character_write_handle.stats.pos = new_pos;
            character_write_handle.stats.rot = new_rot;

            let mut characters_to_interact = Vec::new();
            for npc_guid in auto_interact_npcs {
                if let Some(npc_read_handle) = characters_read.get(&npc_guid) {
                    if npc_read_handle.stats.auto_interact_radius > 0.0 {
                        let distance_now = distance3(
                            character_write_handle.stats.pos.x,
                            character_write_handle.stats.pos.y,
                            character_write_handle.stats.pos.z,
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

            Ok(characters_to_interact)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn move_character(
        pos_update: UpdatePlayerPosition,
        should_teleport: bool,
        game_server: &GameServer,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let moved_character_guid = pos_update.guid;
        let new_pos = Pos {
            x: pos_update.pos_x,
            y: pos_update.pos_y,
            z: pos_update.pos_z,
            w: 1.0,
        };
        let new_rot = Pos {
            x: pos_update.rot_x,
            y: pos_update.rot_y,
            z: pos_update.rot_z,
            w: 0.0,
        };
        let new_chunk = Character::chunk(new_pos.x, new_pos.z);

        let (character_exists, same_chunk, mut broadcasts, npcs_to_interact_if_same_chunk) = game_server
            .lock_enforcer()
            .read_characters(|characters_table_read_handle| {
                let (instance_guid_if_exists, same_chunk, auto_interact_npcs, write_guids) = if let Some((_, instance_guid, old_chunk)) =
                    characters_table_read_handle.index(moved_character_guid)
                {
                    let same_chunk = old_chunk == new_chunk;
                    if same_chunk {
                        let auto_interactable_npcs: Vec<u64> = characters_table_read_handle
                            .keys_by_index((
                                CharacterCategory::NpcAutoInteractEnabled,
                                instance_guid,
                                new_chunk,
                            ))
                            .collect();

                        (Some(instance_guid), true, auto_interactable_npcs, vec![moved_character_guid])
                    } else {
                        (Some(instance_guid), false, Vec::new(), Vec::new())
                    }
                } else {
                    (None, false, Vec::new(), Vec::new())
                };

                CharacterLockRequest {
                    read_guids: auto_interact_npcs.clone(),
                    write_guids,
                    character_consumer: move |characters_table_read_handle, characters_read, characters_write, _| {
                        if let Some(instance_guid) = instance_guid_if_exists {
                            if same_chunk {
                                let mut broadcasts = Vec::new();
                                let filtered_npcs_to_interact = ZoneInstance::move_character_with_locks(
                                    auto_interact_npcs,
                                    characters_read,
                                    characters_write,
                                    moved_character_guid,
                                    new_pos,
                                    new_rot,
                                )?;

                                // We don't return this value when the chunks are different, as players could change between when
                                // we release the read lock and acquire the write lock
                                if let Ok(moved_player_guid) = shorten_player_guid(moved_character_guid) {
                                    let other_players_nearby = ZoneInstance::other_players_nearby(
                                        Some(moved_player_guid),
                                        new_chunk,
                                        instance_guid,
                                        characters_table_read_handle,
                                    )?;
                                    broadcasts.push(Broadcast::Multi(other_players_nearby, vec![GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: pos_update
                                    })?]));
                                }
                                Ok::<(bool, bool, Vec<Broadcast>, Vec<u64>), ProcessPacketError>((true, same_chunk, broadcasts, filtered_npcs_to_interact,))
                            } else {
                                Ok((true, same_chunk, Vec::new(), Vec::new()))
                            }
                        } else {
                            Ok((false, same_chunk, Vec::new(), Vec::new()))
                        }
                    },
                }
            })?;

        if !character_exists {
            return Ok(broadcasts);
        }

        let npcs_to_interact_with = if same_chunk {
            npcs_to_interact_if_same_chunk
        } else {
            game_server
                .lock_enforcer()
                .write_characters(|characters_table_write_handle, _| {
                    if let Some((character, (category, instance_guid, old_chunk))) =
                        characters_table_write_handle.remove(moved_character_guid)
                    {
                        // Build characters read and write maps
                        let mut characters_write: BTreeMap<
                            u64,
                            parking_lot::lock_api::RwLockWriteGuard<
                                parking_lot::RawRwLock,
                                Character,
                            >,
                        > = BTreeMap::new();
                        characters_write.insert(moved_character_guid, character.write());

                        let (character_diffs, mut characters_read) = diff_character_handles!(
                            instance_guid,
                            old_chunk,
                            new_chunk,
                            characters_table_write_handle,
                            moved_character_guid
                        );

                        let auto_interactable_npcs: Vec<u64> = characters_table_write_handle
                            .keys_by_index((
                                CharacterCategory::NpcAutoInteractEnabled,
                                instance_guid,
                                new_chunk,
                            ))
                            .collect();
                        for npc_guid in auto_interactable_npcs {
                            if let Some(npc) = characters_table_write_handle.get(npc_guid) {
                                characters_read.insert(npc_guid, npc.read());
                            }
                        }

                        broadcasts.append(&mut ZoneInstance::diff_character_broadcasts(
                            moved_character_guid,
                            character_diffs,
                            &characters_read,
                            game_server.mounts(),
                            game_server.items(),
                            game_server.customizations(),
                        )?);

                        let characters_to_interact = ZoneInstance::move_character_with_locks(
                            npcs_to_interact_if_same_chunk,
                            characters_read,
                            characters_write,
                            moved_character_guid,
                            new_pos,
                            new_rot,
                        )?;
                        characters_table_write_handle.insert_lock(
                            moved_character_guid,
                            (category, instance_guid, new_chunk),
                            character,
                        );

                        if let Ok(moved_player_guid) = shorten_player_guid(moved_character_guid) {
                            let other_players_nearby = ZoneInstance::other_players_nearby(
                                Some(moved_player_guid),
                                new_chunk,
                                instance_guid,
                                characters_table_write_handle,
                            )?;
                            broadcasts.push(Broadcast::Multi(
                                other_players_nearby,
                                vec![GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: pos_update,
                                })?],
                            ));
                        }

                        Ok::<Vec<u64>, ProcessPacketError>(characters_to_interact)
                    } else {
                        Ok(Vec::new())
                    }
                })?
        };

        for character_guid in npcs_to_interact_with {
            let interact_request = SelectPlayer {
                requester: moved_character_guid,
                target: character_guid,
            };
            broadcasts.append(&mut interact_with_character(interact_request, game_server)?);
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
                    })?],
                ));
            }
        }

        Ok(broadcasts)
    }
}

impl ZoneConfig {
    fn into_zone_instances(self) -> (ZoneTemplate, Vec<ZoneInstance>) {
        let mut characters = Vec::new();

        let mut index = 0;

        {
            for ambient_npc in self.ambient_npcs {
                characters.push(NpcTemplate {
                    key: ambient_npc.base_npc.key.clone(),
                    discriminant: AMBIENT_NPC_DISCRIMINANT,
                    index,
                    pos: Pos {
                        x: ambient_npc.base_npc.pos_x,
                        y: ambient_npc.base_npc.pos_y,
                        z: ambient_npc.base_npc.pos_z,
                        w: ambient_npc.base_npc.pos_w,
                    },
                    rot: Pos {
                        x: ambient_npc.base_npc.rot_x,
                        y: ambient_npc.base_npc.rot_y,
                        z: ambient_npc.base_npc.rot_z,
                        w: ambient_npc.base_npc.rot_w,
                    },
                    scale: ambient_npc.base_npc.scale,
                    tickable_procedures: ambient_npc.base_npc.tickable_procedures.clone(),
                    first_possible_procedures: ambient_npc
                        .base_npc
                        .first_possible_procedures
                        .clone(),
                    synchronize_with: ambient_npc.base_npc.synchronize_with.clone(),
                    animation_id: ambient_npc.base_npc.active_animation_slot,
                    cursor: ambient_npc.base_npc.cursor,
                    character_type: CharacterType::AmbientNpc(ambient_npc.into()),
                    mount_id: None,
                    interact_radius: self.interact_radius,
                    auto_interact_radius: 0.0,
                    wield_type: WieldType::None,
                });
                index += 1;
            }

            for door in self.doors {
                characters.push(NpcTemplate {
                    key: door.base_npc.key.clone(),
                    discriminant: AMBIENT_NPC_DISCRIMINANT,
                    index,
                    pos: Pos {
                        x: door.base_npc.pos_x,
                        y: door.base_npc.pos_y,
                        z: door.base_npc.pos_z,
                        w: door.base_npc.pos_w,
                    },
                    rot: Pos {
                        x: door.base_npc.rot_x,
                        y: door.base_npc.rot_y,
                        z: door.base_npc.rot_z,
                        w: door.base_npc.rot_w,
                    },
                    scale: door.base_npc.scale,
                    tickable_procedures: door.base_npc.tickable_procedures.clone(),
                    first_possible_procedures: door.base_npc.first_possible_procedures.clone(),
                    synchronize_with: door.base_npc.synchronize_with.clone(),
                    animation_id: door.base_npc.active_animation_slot,
                    cursor: door.base_npc.cursor,
                    character_type: CharacterType::Door(door.into()),
                    mount_id: None,
                    interact_radius: self.interact_radius,
                    auto_interact_radius: self.door_auto_interact_radius,
                    wield_type: WieldType::None,
                });
                index += 1;
            }

            for transport in self.transports {
                characters.push(NpcTemplate {
                    key: transport.base_npc.key.clone(),
                    discriminant: AMBIENT_NPC_DISCRIMINANT,
                    index,
                    pos: Pos {
                        x: transport.base_npc.pos_x,
                        y: transport.base_npc.pos_y,
                        z: transport.base_npc.pos_z,
                        w: transport.base_npc.pos_w,
                    },
                    rot: Pos {
                        x: transport.base_npc.rot_x,
                        y: transport.base_npc.rot_y,
                        z: transport.base_npc.rot_z,
                        w: transport.base_npc.rot_w,
                    },
                    scale: transport.base_npc.scale,
                    tickable_procedures: transport.base_npc.tickable_procedures.clone(),
                    first_possible_procedures: transport.base_npc.first_possible_procedures.clone(),
                    synchronize_with: transport.base_npc.synchronize_with.clone(),
                    animation_id: transport.base_npc.active_animation_slot,
                    cursor: transport.base_npc.cursor,
                    character_type: CharacterType::Transport(transport.into()),
                    mount_id: None,
                    interact_radius: self.interact_radius,
                    auto_interact_radius: 0.0,
                    wield_type: WieldType::None,
                });
                index += 1;
            }
        }

        let template = ZoneTemplate {
            guid: self.guid,
            template_name: self.template_name,
            max_players: self.max_players,
            template_icon: self.template_icon.unwrap_or(0),
            asset_name: self.asset_name.clone(),
            default_spawn_pos: Pos {
                x: self.spawn_pos_x,
                y: self.spawn_pos_y,
                z: self.spawn_pos_z,
                w: self.spawn_pos_w,
            },
            default_spawn_rot: Pos {
                x: self.spawn_rot_x,
                y: self.spawn_rot_y,
                z: self.spawn_rot_z,
                w: self.spawn_rot_w,
            },
            default_spawn_sky: self.spawn_sky.clone().unwrap_or("".to_string()),
            speed: self.speed,
            jump_height_multiplier: self.jump_height_multiplier,
            gravity_multiplier: self.gravity_multiplier,
            hide_ui: self.hide_ui,
            is_combat: self.is_combat,
            characters,
            seconds_per_day: self.seconds_per_day,
        };

        (template, Vec::new())
    }
}

type ZoneTemplateMap = BTreeMap<u8, ZoneTemplate>;
pub fn load_zones(
    config_dir: &Path,
) -> Result<(ZoneTemplateMap, GuidTable<u64, ZoneInstance, u8>), Error> {
    let mut file = File::open(config_dir.join("zones.json"))?;
    let zone_configs: Vec<ZoneConfig> = serde_json::from_reader(&mut file)?;

    let mut templates = BTreeMap::new();
    let zones = GuidTable::new();
    {
        let mut zones_write_handle = zones.write();
        for zone_config in zone_configs {
            let (template, zones) = zone_config.into_zone_instances();
            let template_guid = Guid::guid(&template);

            if templates.insert(template_guid, template).is_some() {
                panic!("Two zone templates have ID {}", template_guid);
            }

            for zone in zones {
                let zone_guid = zone.guid();
                if zones_write_handle.insert(zone).is_some() {
                    panic!("Two zone templates have ID {}", zone_guid);
                }
            }
        }
    }

    Ok((templates, zones))
}

pub fn enter_zone(
    characters_table_write_handle: &mut CharacterTableWriteHandle,
    player: u32,
    destination_read_handle: &RwLockReadGuard<ZoneInstance>,
    destination_pos: Option<Pos>,
    destination_rot: Option<Pos>,
    update_previous_location: bool,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let destination_pos = destination_pos.unwrap_or(destination_read_handle.default_spawn_pos);
    let destination_rot = destination_rot.unwrap_or(destination_read_handle.default_spawn_rot);

    // Perform fallible operations before we update player data to avoid an inconsistent state
    let broadcasts = prepare_init_zone_packets(
        player,
        destination_read_handle,
        destination_pos,
        destination_rot,
    )?;

    let character = characters_table_write_handle.remove(player_guid(player));
    if let Some((character, (character_category, _, _))) = character {
        let mut character_write_handle = character.write();
        let previous_zone_template_guid =
            zone_template_guid(character_write_handle.stats.instance_guid);
        let previous_pos = character_write_handle.stats.pos;
        let previous_rot = character_write_handle.stats.rot;

        if let CharacterType::Player(ref mut player) =
            &mut character_write_handle.stats.character_type
        {
            player.ready = false;

            if update_previous_location {
                player.previous_location = PreviousLocation {
                    template_guid: previous_zone_template_guid,
                    pos: previous_pos,
                    rot: previous_rot,
                }
            }
        }
        character_write_handle.stats.instance_guid = destination_read_handle.guid;
        character_write_handle.stats.pos = destination_pos;
        character_write_handle.stats.rot = destination_rot;

        drop(character_write_handle);
        characters_table_write_handle.insert_lock(
            player_guid(player),
            (
                character_category,
                destination_read_handle.guid,
                Character::chunk(
                    destination_read_handle.default_spawn_pos.x,
                    destination_read_handle.default_spawn_pos.z,
                ),
            ),
            character,
        );
    }

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
    })?);

    packets.append(&mut destination.send_self()?);
    packets.push(GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: ExecuteScriptWithParams {
            script_name: format!(
                "CombatHandler.{}",
                if destination.is_combat {
                    "show"
                } else {
                    "hide"
                }
            ),
            params: vec![],
        },
    })?);

    if let Some(house) = &destination.house_data {
        packets.append(&mut prepare_init_house_packets(player, destination, house)?);
    }

    Ok(vec![Broadcast::Single(player, packets)])
}

#[macro_export]
macro_rules! teleport_to_zone {
    ($characters_table_write_handle:expr, $player:expr,
     $destination_read_handle:expr, $destination_pos:expr, $destination_rot:expr, $mounts:expr,
     $update_previous_location:expr$(,)?) => {{
        let character = $crate::game_server::handlers::guid::GuidTableHandle::get(
            $characters_table_write_handle,
            player_guid($player),
        );

        let mut broadcasts = Vec::new();
        if let Some(character_lock) = character {
            broadcasts.append(&mut $crate::game_server::handlers::mount::reply_dismount(
                $player,
                $destination_read_handle,
                &mut character_lock.write(),
                $mounts,
            )?);
        }

        broadcasts.append(&mut $crate::game_server::handlers::zone::enter_zone(
            $characters_table_write_handle,
            $player,
            $destination_read_handle,
            $destination_pos,
            $destination_rot,
            $update_previous_location,
        )?);

        Ok(broadcasts)
    }};
}

pub fn interact_with_character(
    request: SelectPlayer,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let requester = shorten_player_guid(request.requester)?;
    let broadcast_supplier: WriteLockingBroadcastSupplier =
        game_server.lock_enforcer().read_characters(|_| {
            CharacterLockRequest {
                read_guids: Vec::new(),
                write_guids: vec![request.requester, request.target],
                character_consumer: move |_, _, mut characters_write, _| {
                    let source_zone_guid;
                    let requester_x;
                    let requester_y;
                    let requester_z;
                    if let Some(requester_read_handle) = characters_write.get(&request.requester) {
                        source_zone_guid = requester_read_handle.stats.instance_guid;
                        requester_x = requester_read_handle.stats.pos.x;
                        requester_y = requester_read_handle.stats.pos.y;
                        requester_z = requester_read_handle.stats.pos.z;
                    } else {
                        return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                    }

                    if let Some(target_read_handle) = characters_write.get_mut(&request.target) {
                        // Ensure the character is close enough to interact
                        let distance = distance3(
                            requester_x,
                            requester_y,
                            requester_z,
                            target_read_handle.stats.pos.x,
                            target_read_handle.stats.pos.y,
                            target_read_handle.stats.pos.z,
                        );
                        if distance > target_read_handle.stats.interact_radius {
                            return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                        }

                        target_read_handle.interact(requester, source_zone_guid)
                    } else {
                        info!(
                            "Received request to interact with unknown NPC {} from {}",
                            request.target, request.requester
                        );
                        coerce_to_broadcast_supplier(|_| Ok(Vec::new()))
                    }
                },
            }
        });

    broadcast_supplier?(game_server)
}

pub fn teleport_within_zone(
    sender: u32,
    destination_pos: Pos,
    destination_rot: Pos,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    Ok(vec![Broadcast::Single(
        sender,
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: Position {
                player_pos: destination_pos,
                rot: destination_rot,
                is_teleport: true,
                unknown2: true,
            },
        })?],
    )])
}

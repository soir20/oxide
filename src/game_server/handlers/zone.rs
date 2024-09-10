use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Error,
    path::Path,
};

use packet_serialize::SerializePacketError;
use parking_lot::RwLockReadGuard;
use serde::Deserialize;

use crate::game_server::{
    packets::{
        client_update::Position,
        command::SelectPlayer,
        housing::BuildArea,
        item::WieldType,
        login::{ClientBeginZoning, ZoneDetails},
        tunnel::TunneledPacket,
        ui::ExecuteScriptWithParams,
        update_position::UpdatePlayerPosition,
        GamePacket, Pos,
    },
    Broadcast, GameServer, ProcessPacketError,
};

use super::{
    character::{
        Character, CharacterCategory, CharacterIndex, CharacterType, Chunk, Door, NpcTemplate,
        PreviousFixture, Transport,
    },
    guid::{Guid, GuidTable, GuidTableWriteHandle, IndexedGuid},
    housing::prepare_init_house_packets,
    lock_enforcer::{
        CharacterLockRequest, CharacterReadGuard, CharacterTableReadHandle,
        CharacterTableWriteHandle, CharacterWriteGuard, ZoneLockRequest,
    },
    mount::MountConfig,
    unique_guid::{
        npc_guid, player_guid, shorten_player_guid, zone_instance_guid, AMBIENT_NPC_DISCRIMINANT,
        FIXTURE_DISCRIMINANT,
    },
};

use crate::game_server::handlers::guid::GuidTableHandle;
use strum::IntoEnumIterator;

#[derive(Deserialize)]
struct ZoneConfig {
    guid: u8,
    instances: u32,
    template_name: u32,
    template_icon: Option<u32>,
    asset_name: String,
    hide_ui: bool,
    force_combat_pose: bool,
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
    doors: Vec<Door>,
    interact_radius: f32,
    door_auto_interact_radius: f32,
    transports: Vec<Transport>,
}

#[derive(Clone)]
pub struct ZoneTemplate {
    guid: u8,
    pub template_name: u32,
    pub template_icon: u32,
    pub asset_name: String,
    pub default_spawn_pos: Pos,
    pub default_spawn_rot: Pos,
    default_spawn_sky: String,
    pub speed: f32,
    pub jump_height_multiplier: f32,
    pub gravity_multiplier: f32,
    hide_ui: bool,
    force_combat_pose: bool,
    is_combat: bool,
    characters: Vec<NpcTemplate>,
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
    pub fn to_zone(
        &self,
        instance_guid: u64,
        house_data: Option<House>,
        global_characters_table: &mut GuidTableWriteHandle<u64, Character, CharacterIndex>,
    ) -> Zone {
        for character_template in self.characters.iter() {
            global_characters_table.insert(character_template.to_character(instance_guid));
        }

        Zone {
            guid: instance_guid,
            template_guid: Guid::guid(self),
            template_name: self.template_name,
            icon: self.template_icon,
            asset_name: self.asset_name.clone(),
            default_spawn_pos: self.default_spawn_pos,
            default_spawn_rot: self.default_spawn_rot,
            default_spawn_sky: self.default_spawn_sky.clone(),
            speed: self.speed,
            jump_height_multiplier: self.jump_height_multiplier,
            gravity_multiplier: self.gravity_multiplier,
            hide_ui: self.hide_ui,
            force_combat_pose: self.force_combat_pose,
            is_combat: self.is_combat,
            house_data,
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

pub struct Zone {
    guid: u64,
    pub template_guid: u8,
    pub template_name: u32,
    pub icon: u32,
    pub asset_name: String,
    pub default_spawn_pos: Pos,
    pub default_spawn_rot: Pos,
    default_spawn_sky: String,
    pub speed: f32,
    pub jump_height_multiplier: f32,
    pub gravity_multiplier: f32,
    hide_ui: bool,
    pub force_combat_pose: bool,
    pub is_combat: bool,
    pub house_data: Option<House>,
}

impl IndexedGuid<u64, u8> for Zone {
    fn guid(&self) -> u64 {
        self.guid
    }

    fn index(&self) -> u8 {
        self.template_guid
    }
}

#[macro_export]
macro_rules! diff_character_handles {
    ($instance_guid:expr, $old_chunk:expr, $new_chunk:expr, $characters_table_write_handle:expr) => {{
        let old_chunks = Zone::nearby_chunks($old_chunk);
        let new_chunks = Zone::nearby_chunks($new_chunk);
        let chunks_to_remove: Vec<&Chunk> = old_chunks.difference(&new_chunks).collect();
        let chunks_to_add: Vec<&Chunk> = new_chunks.difference(&old_chunks).collect();

        let mut guids = BTreeMap::new();
        for category in CharacterCategory::iter() {
            for chunk in chunks_to_remove.iter() {
                for guid in $characters_table_write_handle.keys_by_index((
                    $instance_guid,
                    **chunk,
                    category,
                )) {
                    guids.insert(guid, false);
                }
            }
        }

        for category in CharacterCategory::iter() {
            for chunk in chunks_to_add.iter() {
                for guid in $characters_table_write_handle.keys_by_index((
                    $instance_guid,
                    **chunk,
                    category,
                )) {
                    guids.insert(guid, true);
                }
            }
        }

        let mut handles = BTreeMap::new();
        for guid in guids.keys() {
            if let Some(character) = $characters_table_write_handle.get(*guid) {
                handles.insert(*guid, character.read());
            }
        }

        (guids, handles)
    }};
}

impl Zone {
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
                0,
                CharacterType::Fixture(guid, fixture.as_current_fixture()),
                None,
                0.0,
                0.0,
                guid,
                WieldType::None,
            ));
        }
        template.to_zone(guid, Some(house), global_characters_table)
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

    pub fn other_players_nearby(sender: u32, chunk: Chunk, instance_guid: u64, characters_table_read_handle: &CharacterTableReadHandle) -> Result<Vec<u32>, ProcessPacketError> {
        let mut guids = Vec::new();

        for chunk in Zone::nearby_chunks(chunk) {
            for guid in characters_table_read_handle.keys_by_index((instance_guid, chunk, CharacterCategory::Player)) {
                if guid != player_guid(sender) {
                    guids.push(shorten_player_guid(guid)?);
                }
            }
        }

        Ok(guids)
    }

    pub fn all_players_nearby(sender: u32, chunk: Chunk, instance_guid: u64, characters_table_read_handle: &CharacterTableReadHandle) -> Result<Vec<u32>, ProcessPacketError> {
        let mut guids = Zone::other_players_nearby(sender, chunk, instance_guid, characters_table_read_handle)?;
        guids.push(sender);
        Ok(guids)
    }

    pub fn diff_character_guids(
        instance_guid: u64,
        old_chunk: Chunk,
        new_chunk: Chunk,
        characters_table_read_handle: &CharacterTableReadHandle,
        requester_guid: u32,
    ) -> BTreeMap<u64, bool> {
        let old_chunks = Zone::nearby_chunks(old_chunk);
        let new_chunks = Zone::nearby_chunks(new_chunk);
        let chunks_to_remove: Vec<&Chunk> = old_chunks.difference(&new_chunks).collect();
        let chunks_to_add: Vec<&Chunk> = new_chunks.difference(&old_chunks).collect();

        let mut guids = BTreeMap::new();
        for category in CharacterCategory::iter() {
            for chunk in chunks_to_remove.iter() {
                for guid in
                    characters_table_read_handle.keys_by_index((instance_guid, **chunk, category))
                {
                    guids.insert(guid, false);
                }
            }
        }

        for category in CharacterCategory::iter() {
            for chunk in chunks_to_add.iter() {
                for guid in
                    characters_table_read_handle.keys_by_index((instance_guid, **chunk, category))
                {
                    guids.insert(guid, true);
                }
            }
        }

        guids.remove(&player_guid(requester_guid));

        guids
    }

    pub fn diff_character_packets(
        character_diffs: &BTreeMap<u64, bool>,
        characters_read: &BTreeMap<u64, CharacterReadGuard<'_>>,
        mount_configs: &BTreeMap<u32, MountConfig>,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let mut packets: Vec<Vec<u8>> = Vec::new();

        for (guid, add) in character_diffs {
            if let Some(character) = characters_read.get(guid) {
                if *add {
                    packets.append(&mut character.add_packets(mount_configs)?);
                } else {
                    packets.append(&mut character.remove_packets(*guid)?);
                }
            }
        }

        Ok(packets)
    }

    fn move_character_with_locks(
        auto_interact_npcs: Vec<u64>,
        characters_read: BTreeMap<u64, CharacterReadGuard<'_>>,
        mut characters_write: BTreeMap<u64, CharacterWriteGuard<'_>>,
        pos_update: UpdatePlayerPosition,
    ) -> Result<Vec<u64>, ProcessPacketError> {
        if let Some(character_write_handle) = characters_write.get_mut(&pos_update.guid) {
            character_write_handle.pos = Pos {
                x: pos_update.pos_x,
                y: pos_update.pos_y,
                z: pos_update.pos_z,
                w: character_write_handle.pos.z,
            };
            character_write_handle.rot = Pos {
                x: pos_update.rot_x,
                y: pos_update.rot_y,
                z: pos_update.rot_z,
                w: character_write_handle.rot.z,
            };
            character_write_handle.state = pos_update.character_state;

            let mut characters_to_interact = Vec::new();
            for npc_guid in auto_interact_npcs {
                if let Some(npc_read_handle) = characters_read.get(&npc_guid) {
                    if npc_read_handle.auto_interact_radius > 0.0 {
                        let distance = distance3(
                            character_write_handle.pos.x,
                            character_write_handle.pos.y,
                            character_write_handle.pos.z,
                            npc_read_handle.pos.x,
                            npc_read_handle.pos.y,
                            npc_read_handle.pos.z,
                        );
                        if distance <= npc_read_handle.auto_interact_radius {
                            characters_to_interact.push(npc_read_handle.guid);
                        }
                    }
                }
            }

            Ok(characters_to_interact)
        } else {
            println!(
                "Received position update from unknown character {}",
                pos_update.guid
            );
            Err(ProcessPacketError::CorruptedPacket)
        }
    }

    pub fn move_character(
        sender: u32,
        pos_update: UpdatePlayerPosition,
        game_server: &GameServer,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let requester = pos_update.guid;
        let pos_update_clone = pos_update.clone();
        let (characters_in_radius_or_all, same_chunk) = game_server
            .lock_enforcer()
            .read_characters(|characters_table_read_handle| {
                let (auto_interact_npcs, same_chunk) = characters_table_read_handle
                    .index(requester)
                    .map(|(instance_guid, old_chunk, _)| {
                        (
                            characters_table_read_handle
                                .keys_by_index((
                                    instance_guid,
                                    old_chunk,
                                    CharacterCategory::NpcAutoInteractEnabled,
                                ))
                                .collect(),
                            old_chunk == Character::chunk(pos_update.pos_x, pos_update.pos_z),
                        )
                    })
                    .unwrap_or((Vec::new(), true));

                CharacterLockRequest {
                    read_guids: auto_interact_npcs.clone(),
                    write_guids: vec![requester],
                    character_consumer: move |_, characters_read, characters_write, _| {
                        if same_chunk {
                            Zone::move_character_with_locks(
                                auto_interact_npcs,
                                characters_read,
                                characters_write,
                                pos_update,
                            )
                            .map(|characters_to_interact| (characters_to_interact, same_chunk))
                        } else {
                            Ok((auto_interact_npcs, same_chunk))
                        }
                    },
                }
            })?;

        let (characters_to_interact, mut broadcasts) = if same_chunk {
            (characters_in_radius_or_all, Vec::new())
        } else {
            game_server
                .lock_enforcer()
                .write_characters(|characters_table_write_handle, _| {
                    if let Some((character, (instance_guid, old_chunk, category))) =
                        characters_table_write_handle.remove(requester)
                    {
                        let mut characters_write: BTreeMap<
                            u64,
                            parking_lot::lock_api::RwLockWriteGuard<
                                parking_lot::RawRwLock,
                                Character,
                            >,
                        > = BTreeMap::new();
                        characters_write.insert(requester, character.write());

                        let new_chunk =
                            Character::chunk(pos_update_clone.pos_x, pos_update_clone.pos_z);

                        let (diff_character_guids, mut characters_read) = diff_character_handles!(
                            instance_guid,
                            old_chunk,
                            new_chunk,
                            characters_table_write_handle
                        );
                        for npc_guid in characters_in_radius_or_all.iter() {
                            if let Some(npc) = characters_table_write_handle.get(*npc_guid) {
                                characters_read.insert(*npc_guid, npc.read());
                            }
                        }

                        let diff_packets = Zone::diff_character_packets(
                            &diff_character_guids,
                            &characters_read,
                            &game_server.mounts,
                        )?;

                        let characters_to_interact = Zone::move_character_with_locks(
                            characters_in_radius_or_all,
                            characters_read,
                            characters_write,
                            pos_update_clone,
                        )?;
                        characters_table_write_handle.insert_lock(
                            requester,
                            (instance_guid, new_chunk, category),
                            character,
                        );

                        let character_diff_broadcast = Broadcast::Single(sender, diff_packets);
                        Ok((characters_to_interact, vec![character_diff_broadcast]))
                    } else {
                        Err(ProcessPacketError::CorruptedPacket)
                    }
                })?
        };

        for character_guid in characters_to_interact {
            let interact_request = SelectPlayer {
                requester,
                target: character_guid,
            };
            broadcasts.append(&mut interact_with_character(interact_request, game_server)?);
        }

        Ok(broadcasts)
    }
}

impl ZoneConfig {
    fn into_zones(
        self,
        global_characters_table: &mut GuidTableWriteHandle<u64, Character, CharacterIndex>,
    ) -> (ZoneTemplate, Vec<Zone>) {
        let mut characters = Vec::new();

        let mut index = 0;

        {
            for door in self.doors {
                characters.push(NpcTemplate {
                    discriminant: AMBIENT_NPC_DISCRIMINANT,
                    index,
                    pos: Pos {
                        x: door.x,
                        y: door.y,
                        z: door.z,
                        w: door.w,
                    },
                    rot: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                    scale: 1.0,
                    state: 0,
                    character_type: CharacterType::Door(door),
                    mount_id: None,
                    interact_radius: self.interact_radius,
                    auto_interact_radius: self.door_auto_interact_radius,
                    wield_type: WieldType::None,
                });
                index += 1;
            }

            for transport in self.transports {
                characters.push(NpcTemplate {
                    discriminant: AMBIENT_NPC_DISCRIMINANT,
                    index,
                    pos: Pos {
                        x: transport.pos_x,
                        y: transport.pos_y,
                        z: transport.pos_z,
                        w: transport.pos_w,
                    },
                    rot: Pos {
                        x: transport.rot_x,
                        y: transport.rot_y,
                        z: transport.rot_z,
                        w: transport.rot_w,
                    },
                    scale: transport.scale.unwrap_or(1.0),
                    state: 0,
                    character_type: CharacterType::Transport(transport),
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
            force_combat_pose: self.force_combat_pose,
            is_combat: self.is_combat,
            characters,
        };

        let mut zones = Vec::new();
        for index in 0..self.instances {
            let instance_guid = zone_instance_guid(index, Guid::guid(&template));

            zones.push(template.to_zone(instance_guid, None, global_characters_table));
        }

        (template, zones)
    }
}

type ZoneTemplateMap = BTreeMap<u8, ZoneTemplate>;
pub fn load_zones(
    config_dir: &Path,
    mut global_characters_table: GuidTableWriteHandle<u64, Character, CharacterIndex>,
) -> Result<(ZoneTemplateMap, GuidTable<u64, Zone, u8>), Error> {
    let mut file = File::open(config_dir.join("zones.json"))?;
    let zone_configs: Vec<ZoneConfig> = serde_json::from_reader(&mut file)?;

    let mut templates = BTreeMap::new();
    let zones = GuidTable::new();
    {
        let mut zones_write_handle = zones.write();
        for zone_config in zone_configs {
            let (template, zones) = zone_config.into_zones(&mut global_characters_table);
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
    destination_read_handle: &RwLockReadGuard<Zone>,
    destination_pos: Option<Pos>,
    destination_rot: Option<Pos>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let destination_pos = destination_pos.unwrap_or(destination_read_handle.default_spawn_pos);
    let destination_rot = destination_rot.unwrap_or(destination_read_handle.default_spawn_rot);

    let character = characters_table_write_handle.remove(player_guid(player));
    if let Some((character, (_, _, character_category))) = character {
        let mut character_write_handle = character.write();
        character_write_handle.instance_guid = destination_read_handle.guid;
        character_write_handle.pos = destination_pos;
        character_write_handle.rot = destination_rot;
        drop(character_write_handle);
        characters_table_write_handle.insert_lock(
            player_guid(player),
            (
                destination_read_handle.guid,
                Character::chunk(
                    destination_read_handle.default_spawn_pos.x,
                    destination_read_handle.default_spawn_pos.z,
                ),
                character_category,
            ),
            character,
        );
    }
    prepare_init_zone_packets(
        player,
        destination_read_handle,
        destination_pos,
        destination_rot,
    )
}

fn prepare_init_zone_packets(
    player: u32,
    destination: &RwLockReadGuard<Zone>,
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
     $destination_read_handle:expr, $destination_pos:expr, $destination_rot:expr, $mounts:expr) => {{
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
        )?);

        Ok(broadcasts)
    }};
}

type PacketSupplier = Result<
    Box<dyn FnOnce(&GameServer) -> Result<Vec<Broadcast>, ProcessPacketError>>,
    ProcessPacketError,
>;
fn coerce_to_packet_supplier(
    f: impl FnOnce(&GameServer) -> Result<Vec<Broadcast>, ProcessPacketError> + 'static,
) -> PacketSupplier {
    Ok(Box::new(f))
}
pub fn interact_with_character(
    request: SelectPlayer,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let requester = shorten_player_guid(request.requester)?;
    let packet_supplier: PacketSupplier = game_server.lock_enforcer().read_characters(|_| {
        CharacterLockRequest {
            read_guids: vec![request.requester, request.target],
            write_guids: Vec::new(),
            character_consumer: move |_, characters_read, _, zones_lock_enforcer| {
                let source_zone_guid;
                let requester_x;
                let requester_y;
                let requester_z;
                if let Some(requester_read_handle) = characters_read.get(&request.requester) {
                    source_zone_guid = requester_read_handle.instance_guid;
                    requester_x = requester_read_handle.pos.x;
                    requester_y = requester_read_handle.pos.y;
                    requester_z = requester_read_handle.pos.z;
                } else {
                    return coerce_to_packet_supplier(|_| Ok(Vec::new()));
                }

                if let Some(target_read_handle) = characters_read.get(&request.target) {
                    // Ensure the character is close enough to interact
                    let distance = distance3(
                        requester_x,
                        requester_y,
                        requester_z,
                        target_read_handle.pos.x,
                        target_read_handle.pos.y,
                        target_read_handle.pos.z,
                    );
                    if distance > target_read_handle.interact_radius {
                        return coerce_to_packet_supplier(|_| Ok(Vec::new()));
                    }

                    // Process interaction based on character's type
                    match &target_read_handle.character_type {
                        CharacterType::Door(door) => {
                            let destination_pos = Pos {
                                x: door.destination_pos_x,
                                y: door.destination_pos_y,
                                z: door.destination_pos_z,
                                w: door.destination_pos_w,
                            };
                            let destination_rot = Pos {
                                x: door.destination_rot_x,
                                y: door.destination_rot_y,
                                z: door.destination_rot_z,
                                w: door.destination_rot_w,
                            };

                            let destination_zone_guid =
                                if let &Some(destination_zone_guid) = &door.destination_zone {
                                    destination_zone_guid
                                } else if let &Some(destination_zone_template) =
                                    &door.destination_zone_template
                                {
                                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                        read_guids: Vec::new(),
                                        write_guids: Vec::new(),
                                        zone_consumer: |zones_table_read_handle, _, _| {
                                            GameServer::any_instance(
                                                zones_table_read_handle,
                                                destination_zone_template,
                                            )
                                        },
                                    })?
                                } else {
                                    source_zone_guid
                                };

                            coerce_to_packet_supplier(move |game_server| {
                                game_server.lock_enforcer().write_characters(
                                    |characters_table_write_handle, zones_lock_enforcer| {
                                        if source_zone_guid != destination_zone_guid {
                                            zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                                read_guids: vec![destination_zone_guid],
                                                write_guids: Vec::new(),
                                                zone_consumer: |_, zones_read, _| {
                                                    if let Some(destination_read_handle) =
                                                        zones_read.get(&destination_zone_guid)
                                                    {
                                                        teleport_to_zone!(
                                                            characters_table_write_handle,
                                                            requester,
                                                            destination_read_handle,
                                                            Some(destination_pos),
                                                            Some(destination_rot),
                                                            game_server.mounts()
                                                        )
                                                    } else {
                                                        Ok(Vec::new())
                                                    }
                                                },
                                            })
                                        } else {
                                            teleport_within_zone(
                                                requester,
                                                destination_pos,
                                                destination_rot,
                                                characters_table_write_handle,
                                                &game_server.mounts,
                                            )
                                        }
                                    },
                                )
                            })
                        }
                        CharacterType::Transport(_) => coerce_to_packet_supplier(move |_| {
                            Ok(vec![Broadcast::Single(requester, show_galaxy_map()?)])
                        }),
                        _ => coerce_to_packet_supplier(|_| Ok(Vec::new())),
                    }
                } else {
                    println!(
                        "Received request to interact with unknown NPC {} from {}",
                        request.target, request.requester
                    );
                    Err(ProcessPacketError::CorruptedPacket)
                }
            },
        }
    });

    packet_supplier?(game_server)
}

pub fn teleport_within_zone(
    sender: u32,
    destination_pos: Pos,
    destination_rot: Pos,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    mount_configs: &BTreeMap<u32, MountConfig>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let mut packets = if let Some((character, (instance_guid, old_chunk, _))) =
        characters_table_write_handle.remove(player_guid(sender))
    {
        let mut character_write_handle = character.write();
        character_write_handle.pos = destination_pos;
        character_write_handle.rot = destination_rot;
        let (_, new_chunk, _) = character_write_handle.index();

        let (diff_character_guids, diff_character_handles) = diff_character_handles!(
            instance_guid,
            old_chunk,
            new_chunk,
            characters_table_write_handle
        );
        let diff_packets = Zone::diff_character_packets(
            &diff_character_guids,
            &diff_character_handles,
            mount_configs,
        );
        drop(diff_character_guids);
        drop(diff_character_handles);

        let guid = character_write_handle.guid();
        let index = character_write_handle.index();
        drop(character_write_handle);

        characters_table_write_handle.insert_lock(guid, index, character);
        diff_packets
    } else {
        println!("Player not in any zone tried to teleport within the zone");
        Err(ProcessPacketError::CorruptedPacket)
    }?;

    packets.push(GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: Position {
            player_pos: destination_pos,
            rot: destination_rot,
            is_teleport: true,
            unknown2: true,
        },
    })?);

    Ok(vec![Broadcast::Single(sender, packets)])
}

fn show_galaxy_map() -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    Ok(vec![GamePacket::serialize(&TunneledPacket {
        unknown1: false,
        inner: ExecuteScriptWithParams {
            script_name: "UIGlobal.ShowGalaxyMap".to_string(),
            params: vec![],
        },
    })?])
}

fn distance3(x1: f32, y1: f32, z1: f32, x2: f32, y2: f32, z2: f32) -> f32 {
    let diff_x = x2 - x1;
    let diff_y = y2 - y1;
    let diff_z = z2 - z1;
    (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt()
}

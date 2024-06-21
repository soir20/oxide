use std::collections::BTreeMap;
use std::fs::File;
use std::io::Error;
use std::path::Path;

use parking_lot::RwLockReadGuard;
use serde::Deserialize;

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::character_guid::{npc_guid, player_guid, shorten_player_guid};
use crate::game_server::client_update_packet::Position;
use crate::game_server::command::SelectPlayer;
use crate::game_server::game_packet::{GamePacket, OpCode, Pos};
use crate::game_server::guid::{
    Guid, GuidTable, GuidTableHandle, GuidTableReadHandle, GuidTableWriteHandle, IndexedGuid, Lock,
};
use crate::game_server::housing::{prepare_init_house_packets, BuildArea};
use crate::game_server::login::{ClientBeginZoning, ZoneDetails};
use crate::game_server::player_update_packet::{
    AddNotifications, AddNpc, BaseAttachmentGroup, Icon, NotificationData, NpcRelevance,
    SingleNotification, SingleNpcRelevance, WeaponAnimation,
};
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::ui::ExecuteScriptWithParams;
use crate::game_server::update_position::UpdatePlayerPosition;
use crate::game_server::{Broadcast, GameServer, ProcessPacketError};
use crate::zone_with_character_read;

#[derive(Clone, Deserialize)]
pub struct Door {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    terrain_object_id: u32,
    destination_pos_x: f32,
    destination_pos_y: f32,
    destination_pos_z: f32,
    destination_pos_w: f32,
    destination_rot_x: f32,
    destination_rot_y: f32,
    destination_rot_z: f32,
    destination_rot_w: f32,
    destination_zone_template: Option<u32>,
    destination_zone: Option<u64>,
}

#[derive(Clone, Deserialize)]
pub struct Transport {
    model_id: Option<u32>,
    name_id: Option<u32>,
    terrain_object_id: Option<u32>,
    scale: Option<f32>,
    pos_x: f32,
    pos_y: f32,
    pos_z: f32,
    pos_w: f32,
    rot_x: f32,
    rot_y: f32,
    rot_z: f32,
    rot_w: f32,
    name_offset_x: Option<f32>,
    name_offset_y: Option<f32>,
    name_offset_z: Option<f32>,
    cursor: u8,
    show_name: bool,
    show_icon: bool,
    large_icon: bool,
    show_hover_description: bool,
}

#[derive(Deserialize)]
struct ZoneConfig {
    guid: u32,
    instances: u32,
    template_name: u32,
    template_icon: Option<u32>,
    asset_name: String,
    hide_ui: bool,
    combat_hud: bool,
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
pub enum CharacterType {
    Door(Door),
    Transport(Transport),
    Player,
}

#[derive(Copy, Clone, Eq, PartialOrd, PartialEq, Ord)]
pub enum CharacterCategory {
    Player,
    NpcAutoInteractEnabled,
    NpcAutoInteractDisabled,
}

#[derive(Clone)]
pub struct Character {
    pub guid: u64,
    pub pos: Pos,
    pub rot: Pos,
    pub state: u8,
    pub character_type: CharacterType,
    pub mount_id: Option<u32>,
    pub interact_radius: f32,
    pub auto_interact_radius: f32,
}

impl IndexedGuid<u64, CharacterCategory> for Character {
    fn guid(&self) -> u64 {
        self.guid
    }

    fn index(&self) -> CharacterCategory {
        match self.character_type {
            CharacterType::Player => CharacterCategory::Player,
            _ => match self.auto_interact_radius > 0.0 {
                true => CharacterCategory::NpcAutoInteractEnabled,
                false => CharacterCategory::NpcAutoInteractDisabled,
            },
        }
    }
}

impl Character {
    pub fn to_packets(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        let packets = match &self.character_type {
            CharacterType::Door(door) => {
                let mut packets = vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: Self::door_packet(self, door),
                })?];
                packets.append(&mut enable_interaction(self.guid, 55)?);
                packets
            }
            CharacterType::Transport(transport) => {
                let mut packets = vec![
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: Self::transport_packet(self, transport),
                    })?,
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: AddNotifications {
                            notifications: vec![SingleNotification {
                                guid: self.guid,
                                unknown1: 0,
                                notification: Some(NotificationData {
                                    unknown1: 0,
                                    icon_id: if transport.large_icon { 46 } else { 37 },
                                    unknown3: 0,
                                    name_id: 0,
                                    unknown4: 0,
                                    hide_icon: !transport.show_icon,
                                    unknown6: 0,
                                }),
                                unknown2: false,
                            }],
                        },
                    })?,
                ];
                packets.append(&mut enable_interaction(self.guid, transport.cursor)?);
                packets
            }
            _ => Vec::new(),
        };

        Ok(packets)
    }

    fn door_packet(character: &Character, door: &Door) -> AddNpc {
        AddNpc {
            guid: character.guid,
            name_id: 0,
            model_id: 0,
            unknown3: false,
            unknown4: 408679,
            unknown5: 13951728,
            unknown6: 1,
            scale: 1.0,
            pos: character.pos,
            rot: character.rot,
            unknown8: 1,
            attachments: vec![],
            is_not_targetable: 1,
            unknown10: 1,
            texture_name: "".to_string(),
            tint_name: "".to_string(),
            tint_id: 0,
            unknown11: true,
            offset_y: 0.0,
            composite_effect: 0,
            weapon_animation: WeaponAnimation::None,
            name_override: "".to_string(),
            hide_name: false,
            name_offset_x: 0.0,
            name_offset_y: 0.0,
            name_offset_z: 0.0,
            terrain_object_id: door.terrain_object_id,
            invisible: false,
            unknown20: 0.0,
            unknown21: false,
            interactable_size_pct: 100,
            unknown23: -1,
            unknown24: -1,
            active_animation_slot: -1,
            unknown26: false,
            ignore_position: false,
            sub_title_id: 0,
            active_animation_slot2: 0,
            head_model_id: 0,
            effects: vec![],
            disable_interact_popup: true,
            unknown33: 0,
            unknown34: false,
            show_health: false,
            hide_despawn_fade: false,
            ignore_rotation_and_shadow: false,
            base_attachment_group: BaseAttachmentGroup {
                unknown1: 0,
                unknown2: "".to_string(),
                unknown3: "".to_string(),
                unknown4: 0,
                unknown5: "".to_string(),
            },
            unknown39: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 0.0,
            },
            unknown40: 0,
            unknown41: -1,
            unknown42: 0,
            collision: true,
            unknown44: 0,
            npc_type: 2,
            unknown46: 0.0,
            target: 0,
            unknown50: vec![],
            rail_id: 0,
            rail_speed: 0.0,
            rail_origin: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 0.0,
            },
            unknown54: 0,
            rail_unknown1: 0.0,
            rail_unknown2: 0.0,
            rail_unknown3: 0.0,
            attachment_group_unknown: "".to_string(),
            unknown59: "".to_string(),
            unknown60: "".to_string(),
            override_terrain_model: false,
            hover_glow: 0,
            hover_description: 0,
            fly_over_effect: 0,
            unknown65: 8,
            unknown66: 0,
            unknown67: 3442,
            disable_move_to_interact: false,
            unknown69: 0.0,
            unknown70: 0.0,
            unknown71: 0,
            icon_id: Icon::None,
        }
    }

    fn transport_packet(character: &Character, transport: &Transport) -> AddNpc {
        AddNpc {
            guid: character.guid,
            name_id: transport.name_id.unwrap_or(0),
            model_id: transport.model_id.unwrap_or(0),
            unknown3: false,
            unknown4: 408679,
            unknown5: 13951728,
            unknown6: 1,
            scale: transport.scale.unwrap_or(1.0),
            pos: character.pos,
            rot: character.rot,
            unknown8: 1,
            attachments: vec![],
            is_not_targetable: 1,
            unknown10: 1,
            texture_name: "".to_string(),
            tint_name: "".to_string(),
            tint_id: 0,
            unknown11: true,
            offset_y: 0.0,
            composite_effect: 0,
            weapon_animation: WeaponAnimation::None,
            name_override: "".to_string(),
            hide_name: !transport.show_name,
            name_offset_x: transport.name_offset_x.unwrap_or(0.0),
            name_offset_y: transport.name_offset_y.unwrap_or(0.0),
            name_offset_z: transport.name_offset_z.unwrap_or(0.0),
            terrain_object_id: transport.terrain_object_id.unwrap_or(0),
            invisible: false,
            unknown20: 0.0,
            unknown21: false,
            interactable_size_pct: 100,
            unknown23: -1,
            unknown24: -1,
            active_animation_slot: -1,
            unknown26: false,
            ignore_position: false,
            sub_title_id: 0,
            active_animation_slot2: 0,
            head_model_id: 0,
            effects: vec![],
            disable_interact_popup: false,
            unknown33: 0,
            unknown34: false,
            show_health: false,
            hide_despawn_fade: false,
            ignore_rotation_and_shadow: false,
            base_attachment_group: BaseAttachmentGroup {
                unknown1: 0,
                unknown2: "".to_string(),
                unknown3: "".to_string(),
                unknown4: 0,
                unknown5: "".to_string(),
            },
            unknown39: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 0.0,
            },
            unknown40: 0,
            unknown41: -1,
            unknown42: 0,
            collision: true,
            unknown44: 0,
            npc_type: 2,
            unknown46: 0.0,
            target: 0,
            unknown50: vec![],
            rail_id: 0,
            rail_speed: 0.0,
            rail_origin: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 0.0,
            },
            unknown54: 0,
            rail_unknown1: 0.0,
            rail_unknown2: 0.0,
            rail_unknown3: 0.0,
            attachment_group_unknown: "".to_string(),
            unknown59: "".to_string(),
            unknown60: "".to_string(),
            override_terrain_model: false,
            hover_glow: 0,
            hover_description: if transport.show_hover_description {
                transport.name_id.unwrap_or(0)
            } else {
                0
            },
            fly_over_effect: 0,
            unknown65: 8,
            unknown66: 0,
            unknown67: 3442,
            disable_move_to_interact: false,
            unknown69: 0.0,
            unknown70: 0.0,
            unknown71: 0,
            icon_id: Icon::None,
        }
    }
}

#[derive(Clone)]
pub struct ZoneTemplate {
    guid: u32,
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
    combat_hud: bool,
    characters: Vec<Character>,
}

impl Guid<u32> for ZoneTemplate {
    fn guid(&self) -> u32 {
        self.guid
    }
}

impl From<&Vec<Character>> for GuidTable<u64, Character, CharacterCategory> {
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

pub struct Fixture {
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
    pub item_def_id: u32,
    pub model_id: u32,
    pub texture_name: String,
}

pub struct House {
    pub owner: u32,
    pub owner_name: String,
    pub custom_name: String,
    pub rating: f32,
    pub total_votes: u32,
    pub fixtures: Vec<Fixture>,
    pub build_areas: Vec<BuildArea>,
    pub is_locked: bool,
    pub is_published: bool,
    pub is_rateable: bool,
}

pub struct Zone {
    guid: u64,
    pub template_guid: u32,
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
    combat_hud: bool,
    characters: GuidTable<u64, Character, CharacterCategory>,
    pub house_data: Option<House>,
}

impl Guid<u64> for Zone {
    fn guid(&self) -> u64 {
        self.guid
    }
}

impl Zone {
    pub fn new_house(guid: u64, template: &ZoneTemplate, house: House) -> Self {
        Zone {
            guid,
            template_guid: Guid::guid(template),
            template_name: template.template_name,
            icon: template.template_icon,
            asset_name: template.asset_name.clone(),
            default_spawn_pos: template.default_spawn_pos,
            default_spawn_rot: template.default_spawn_rot,
            default_spawn_sky: template.default_spawn_sky.clone(),
            speed: template.speed,
            jump_height_multiplier: template.jump_height_multiplier,
            gravity_multiplier: template.gravity_multiplier,
            hide_ui: template.hide_ui,
            combat_hud: template.combat_hud,
            characters:
                <GuidTable<u64, Character, CharacterCategory> as From<&Vec<Character>>>::from(
                    &template.characters,
                ),
            house_data: Some(house),
        }
    }

    pub fn send_self(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        Ok(vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: ZoneDetails {
                name: self.asset_name.clone(),
                zone_type: 2,
                hide_ui: self.hide_ui,
                combat_hud: self.combat_hud,
                sky_definition_file_name: self.default_spawn_sky.clone(),
                zoom_out: false,
                unknown7: 0,
                unknown8: 0,
            },
        })?])
    }

    pub fn send_characters(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        let mut packets = Vec::new();
        for character in self.read_characters().values() {
            packets.append(&mut character.read().to_packets()?);
        }

        Ok(packets)
    }

    pub fn read_characters(&self) -> GuidTableReadHandle<u64, Character, CharacterCategory> {
        self.characters.read()
    }

    pub fn write_characters(&self) -> GuidTableWriteHandle<u64, Character, CharacterCategory> {
        self.characters.write()
    }

    pub fn move_character(
        characters: GuidTableReadHandle<u64, Character, CharacterCategory>,
        pos_update: UpdatePlayerPosition,
        game_server: &GameServer,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let possible_character = characters.get(pos_update.guid);
        let mut characters_to_interact = Vec::new();

        if let Some(character) = possible_character {
            let mut write_handle = character.write();
            write_handle.pos = Pos {
                x: pos_update.pos_x,
                y: pos_update.pos_y,
                z: pos_update.pos_z,
                w: write_handle.pos.z,
            };
            write_handle.rot = Pos {
                x: pos_update.rot_x,
                y: pos_update.rot_y,
                z: pos_update.rot_z,
                w: write_handle.rot.z,
            };
            write_handle.state = pos_update.character_state;
            drop(write_handle);

            let read_handle = character.read();
            for character in characters.values_by_index(CharacterCategory::NpcAutoInteractEnabled) {
                let other_read_handle = character.read();
                if other_read_handle.auto_interact_radius > 0.0 {
                    let distance = distance3(
                        read_handle.pos.x,
                        read_handle.pos.y,
                        read_handle.pos.z,
                        other_read_handle.pos.x,
                        other_read_handle.pos.y,
                        other_read_handle.pos.z,
                    );
                    if distance <= other_read_handle.auto_interact_radius {
                        characters_to_interact.push(other_read_handle.guid);
                    }
                }
            }
        } else {
            println!(
                "Received position update from unknown character {}",
                pos_update.guid
            );
            return Err(ProcessPacketError::CorruptedPacket);
        }

        drop(characters);

        let mut broadcasts = Vec::new();
        for character_guid in characters_to_interact {
            let interact_request = SelectPlayer {
                requester: pos_update.guid,
                target: character_guid,
            };
            broadcasts.append(&mut interact_with_character(interact_request, game_server)?);
        }

        Ok(broadcasts)
    }
}

pub fn instance_guid(index: u32, template_guid: u32) -> u64 {
    ((index as u64) << 32) | (template_guid as u64)
}

pub fn template_guid(instance_guid: u64) -> Result<u32, ProcessPacketError> {
    if instance_guid > u32::MAX as u64 {
        Err(ProcessPacketError::CorruptedPacket)
    } else {
        Ok(instance_guid as u32)
    }
}

impl From<ZoneConfig> for (ZoneTemplate, Vec<Zone>) {
    fn from(zone_config: ZoneConfig) -> Self {
        let mut characters = Vec::new();

        // Set the first bit for NPC guids to avoid player GUID conflicts
        let mut index = 0;

        {
            for door in zone_config.doors {
                characters.push(Character {
                    guid: npc_guid(index),
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
                    state: 0,
                    character_type: CharacterType::Door(door),
                    mount_id: None,
                    interact_radius: zone_config.interact_radius,
                    auto_interact_radius: zone_config.door_auto_interact_radius,
                });
                index += 1;
            }

            for transport in zone_config.transports {
                characters.push(Character {
                    guid: npc_guid(index),
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
                    state: 0,
                    character_type: CharacterType::Transport(transport),
                    mount_id: None,
                    interact_radius: zone_config.interact_radius,
                    auto_interact_radius: 0.0,
                });
                index += 1;
            }
        }

        let template = ZoneTemplate {
            guid: zone_config.guid,
            template_name: zone_config.template_name,
            template_icon: zone_config.template_icon.unwrap_or(0),
            asset_name: zone_config.asset_name,
            default_spawn_pos: Pos {
                x: zone_config.spawn_pos_x,
                y: zone_config.spawn_pos_y,
                z: zone_config.spawn_pos_z,
                w: zone_config.spawn_pos_w,
            },
            default_spawn_rot: Pos {
                x: zone_config.spawn_rot_x,
                y: zone_config.spawn_rot_y,
                z: zone_config.spawn_rot_z,
                w: zone_config.spawn_rot_w,
            },
            default_spawn_sky: zone_config.spawn_sky.unwrap_or("".to_string()),
            speed: zone_config.speed,
            jump_height_multiplier: zone_config.jump_height_multiplier,
            gravity_multiplier: zone_config.gravity_multiplier,
            hide_ui: zone_config.hide_ui,
            combat_hud: zone_config.combat_hud,
            characters,
        };

        let mut zones = Vec::new();
        for index in 0..zone_config.instances {
            let instance_guid = instance_guid(index, Guid::guid(&template));

            zones.push(Zone {
                guid: instance_guid,
                template_guid: Guid::guid(&template),
                template_name: template.template_name,
                icon: template.template_icon,
                asset_name: template.asset_name.clone(),
                default_spawn_pos: template.default_spawn_pos,
                default_spawn_rot: template.default_spawn_rot,
                default_spawn_sky: template.default_spawn_sky.clone(),
                speed: template.speed,
                jump_height_multiplier: template.jump_height_multiplier,
                gravity_multiplier: template.gravity_multiplier,
                hide_ui: template.hide_ui,
                combat_hud: template.combat_hud,
                characters: <GuidTable<u64, Character, CharacterCategory> as From<
                    &Vec<Character>,
                >>::from(&template.characters),
                house_data: None,
            });
        }

        (template, zones)
    }
}

type ZoneTemplateMap = BTreeMap<u32, ZoneTemplate>;
pub fn load_zones(config_dir: &Path) -> Result<(ZoneTemplateMap, GuidTable<u64, Zone>), Error> {
    let mut file = File::open(config_dir.join("zones.json"))?;
    let zone_configs: Vec<ZoneConfig> = serde_json::from_reader(&mut file)?;

    let mut templates = BTreeMap::new();
    let zones = GuidTable::new();
    {
        let mut zones_write_handle = zones.write();
        for zone_config in zone_configs {
            let (template, zones) =
                <(ZoneTemplate, Vec<Zone>) as From<ZoneConfig>>::from(zone_config);
            let template_guid = Guid::guid(&template);

            if templates.insert(template_guid, template).is_some() {
                panic!("Two zone templates have ID {}", template_guid);
            }

            for zone in zones {
                let zone_guid = Guid::guid(&zone);
                if zones_write_handle.insert(zone).is_some() {
                    panic!("Two zone templates have ID {}", zone_guid);
                }
            }
        }
    }

    Ok((templates, zones))
}

pub fn enter_zone(
    character: Option<(Lock<Character>, CharacterCategory)>,
    player: u32,
    destination_read_handle: RwLockReadGuard<Zone>,
    destination_pos: Option<Pos>,
    destination_rot: Option<Pos>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let destination_pos = destination_pos.unwrap_or(destination_read_handle.default_spawn_pos);
    let destination_rot = destination_rot.unwrap_or(destination_read_handle.default_spawn_rot);
    if let Some((character, index)) = character {
        let mut characters = destination_read_handle.write_characters();
        characters.insert_lock(player_guid(player), index, character);
        drop(characters);
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
    destination: RwLockReadGuard<Zone>,
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
                if destination.combat_hud {
                    "show"
                } else {
                    "hide"
                }
            ),
            params: vec![],
        },
    })?);

    if let Some(house) = &destination.house_data {
        packets.append(&mut prepare_init_house_packets(
            player,
            &destination,
            house,
        )?);
    }

    Ok(vec![Broadcast::Single(player, packets)])
}

#[macro_export]
macro_rules! teleport_to_zone {
    ($zones:expr, $source_zone:expr, $source_zone_characters:expr, $player:expr,
     $destination_zone_guid:expr, $destination_pos:expr, $destination_rot:expr, $mounts:expr) => {{
        let character = $source_zone_characters.remove(player_guid($player));
        drop($source_zone_characters);
        drop($source_zone);

        if let Some(destination_zone) = $zones.get($destination_zone_guid) {
            let destination_read_handle = destination_zone.read();
            let mut broadcasts = Vec::new();
            if let Some((character, _)) = &character {
                broadcasts.append(&mut $crate::game_server::mount::reply_dismount(
                    $player,
                    &destination_read_handle,
                    &mut character.write(),
                    $mounts,
                )?);
            }

            broadcasts.append(&mut $crate::game_server::zone::enter_zone(
                character,
                $player,
                destination_read_handle,
                $destination_pos,
                $destination_rot,
            )?);

            Ok(broadcasts)
        } else {
            Err(ProcessPacketError::CorruptedPacket)
        }
    }};
}

pub fn interact_with_character(
    request: SelectPlayer,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let zones = game_server.read_zones();
    zone_with_character_read!(
        zones.values(),
        request.requester,
        |source_zone_read_handle, characters| {
            let source_zone_guid = <Zone as Guid<u64>>::guid(&source_zone_read_handle);
            let requester = shorten_player_guid(request.requester)?;
            let requester_x;
            let requester_y;
            let requester_z;
            if let Some(requester) = characters.get(request.requester) {
                let requester_read_handle = requester.read();
                requester_x = requester_read_handle.pos.x;
                requester_y = requester_read_handle.pos.y;
                requester_z = requester_read_handle.pos.z;
            } else {
                return Ok(Vec::new());
            }

            if let Some(target) = characters.get(request.target) {
                let target_read_handle = target.read();

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
                    return Ok(Vec::new());
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
                                GameServer::any_instance(&zones, destination_zone_template)?
                            } else {
                                source_zone_guid
                            };
                        drop(target_read_handle);
                        drop(characters);

                        let mut characters = source_zone_read_handle.write_characters();
                        if source_zone_guid != destination_zone_guid {
                            teleport_to_zone!(
                                &zones,
                                source_zone_read_handle,
                                characters,
                                requester,
                                destination_zone_guid,
                                Some(destination_pos),
                                Some(destination_rot),
                                game_server.mounts()
                            )
                        } else {
                            teleport_within_zone(requester, destination_pos, destination_rot)
                        }
                    }
                    CharacterType::Transport(_) => {
                        Ok(vec![Broadcast::Single(requester, show_galaxy_map()?)])
                    }
                    _ => Ok(Vec::new()),
                }
            } else {
                println!(
                    "Received request to interact with unknown NPC {} from {}",
                    request.target, request.requester
                );
                Err(ProcessPacketError::CorruptedPacket)
            }
        }
    )?
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

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneTeleportRequest {
    pub destination_guid: u32,
}

impl GamePacket for ZoneTeleportRequest {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::ZoneTeleportRequest;
}

fn enable_interaction(guid: u64, cursor: u8) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    Ok(vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: NpcRelevance {
            new_states: vec![SingleNpcRelevance {
                guid,
                cursor: Some(cursor),
                unknown1: false,
            }],
        },
    })?])
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

use std::collections::{BTreeMap, BTreeSet};

use packet_serialize::SerializePacketError;
use serde::Deserialize;
use strum::EnumIter;

use crate::game_server::{
    packets::{
        item::{BaseAttachmentGroup, EquipmentSlot, WieldType},
        player_data::EquippedItem,
        player_update::{
            AddNotifications, AddNpc, CustomizationSlot, Icon, NotificationData, NpcRelevance,
            RemoveStandard, SingleNotification, SingleNpcRelevance,
        },
        tunnel::TunneledPacket,
        GamePacket, Pos,
    },
    GameServer, ProcessPacketError,
};

use super::{
    guid::IndexedGuid,
    housing::fixture_packets,
    inventory::wield_type_from_slot,
    mount::{spawn_mount_npc, MountConfig},
    unique_guid::{mount_guid, npc_guid, player_guid, shorten_player_guid},
};

#[derive(Clone, Deserialize)]
pub struct Door {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
    pub terrain_object_id: u32,
    pub destination_pos_x: f32,
    pub destination_pos_y: f32,
    pub destination_pos_z: f32,
    pub destination_pos_w: f32,
    pub destination_rot_x: f32,
    pub destination_rot_y: f32,
    pub destination_rot_z: f32,
    pub destination_rot_w: f32,
    pub destination_zone_template: Option<u8>,
    pub destination_zone: Option<u64>,
}

#[derive(Clone, Deserialize)]
pub struct Transport {
    pub model_id: Option<u32>,
    pub name_id: Option<u32>,
    pub terrain_object_id: Option<u32>,
    pub scale: Option<f32>,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub pos_w: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub rot_w: f32,
    pub name_offset_x: Option<f32>,
    pub name_offset_y: Option<f32>,
    pub name_offset_z: Option<f32>,
    pub cursor: u8,
    pub show_name: bool,
    pub show_icon: bool,
    pub large_icon: bool,
    pub show_hover_description: bool,
}

#[derive(Clone)]
pub struct BattleClass {
    pub items: BTreeMap<EquipmentSlot, EquippedItem>,
}

#[derive(Clone)]
pub struct Player {
    pub member: bool,
    pub credits: u32,
    pub battle_classes: BTreeMap<u32, BattleClass>,
    pub active_battle_class: u32,
    pub inventory: BTreeSet<u32>,
    pub customizations: BTreeMap<CustomizationSlot, u32>,
}

pub struct PreviousFixture {
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
    pub item_def_id: u32,
    pub model_id: u32,
    pub texture_name: String,
}

impl PreviousFixture {
    pub fn as_current_fixture(&self) -> CurrentFixture {
        CurrentFixture {
            item_def_id: self.item_def_id,
            model_id: self.model_id,
            texture_name: self.texture_name.clone(),
        }
    }
}

#[derive(Clone)]
pub struct CurrentFixture {
    pub item_def_id: u32,
    pub model_id: u32,
    pub texture_name: String,
}

#[derive(Clone)]
pub enum CharacterType {
    Door(Door),
    Transport(Transport),
    Player(Box<Player>),
    Fixture(u64, CurrentFixture),
}

#[derive(Copy, Clone, Eq, EnumIter, PartialOrd, PartialEq, Ord)]
pub enum CharacterCategory {
    Player,
    NpcAutoInteractEnabled,
    NpcAutoInteractDisabled,
}

#[derive(Clone)]
pub struct NpcTemplate {
    pub discriminant: u8,
    pub index: u16,
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
    pub state: u8,
    pub character_type: CharacterType,
    pub mount_id: Option<u32>,
    pub interact_radius: f32,
    pub auto_interact_radius: f32,
    pub wield_type: WieldType,
}

impl NpcTemplate {
    pub fn to_character(&self, instance_guid: u64) -> Character {
        Character {
            guid: npc_guid(self.discriminant, instance_guid, self.index),
            pos: self.pos,
            rot: self.rot,
            scale: self.scale,
            state: self.state,
            character_type: self.character_type.clone(),
            mount_id: self.mount_id,
            interact_radius: self.interact_radius,
            auto_interact_radius: self.auto_interact_radius,
            instance_guid,
            wield_type: (self.wield_type, self.wield_type.holster()),
            holstered: false,
        }
    }
}

pub type Chunk = (i32, i32);
pub type CharacterIndex = (u64, Chunk, CharacterCategory);

#[derive(Clone)]
pub struct Character {
    pub guid: u64,
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
    pub state: u8,
    pub character_type: CharacterType,
    pub mount_id: Option<u32>,
    pub interact_radius: f32,
    pub auto_interact_radius: f32,
    pub instance_guid: u64,
    wield_type: (WieldType, WieldType),
    holstered: bool,
}

impl IndexedGuid<u64, CharacterIndex> for Character {
    fn guid(&self) -> u64 {
        self.guid
    }

    fn index(&self) -> CharacterIndex {
        (
            self.instance_guid,
            Character::chunk(self.pos.x, self.pos.z),
            match self.character_type {
                CharacterType::Player(_) => CharacterCategory::Player,
                _ => match self.auto_interact_radius > 0.0 {
                    true => CharacterCategory::NpcAutoInteractEnabled,
                    false => CharacterCategory::NpcAutoInteractDisabled,
                },
            },
        )
    }
}

impl Character {
    pub const MIN_CHUNK: (i32, i32) = (i32::MIN, i32::MIN);
    const CHUNK_SIZE: f32 = 200.0;

    pub fn new(
        guid: u64,
        pos: Pos,
        rot: Pos,
        scale: f32,
        state: u8,
        character_type: CharacterType,
        mount_id: Option<u32>,
        interact_radius: f32,
        auto_interact_radius: f32,
        instance_guid: u64,
        wield_type: WieldType,
    ) -> Character {
        Character {
            guid,
            pos,
            rot,
            scale,
            state,
            character_type,
            mount_id,
            interact_radius,
            auto_interact_radius,
            instance_guid,
            wield_type: (wield_type, wield_type.holster()),
            holstered: false,
        }
    }

    pub fn from_player(
        guid: u32,
        pos: Pos,
        rot: Pos,
        instance_guid: u64,
        data: Player,
        game_server: &GameServer,
    ) -> Self {
        let wield_type = data
            .battle_classes
            .get(&data.active_battle_class)
            .map(|battle_class| {
                let primary_wield_type = wield_type_from_slot(
                    &battle_class.items,
                    EquipmentSlot::PrimaryWeapon,
                    game_server,
                );
                let secondary_wield_type = wield_type_from_slot(
                    &battle_class.items,
                    EquipmentSlot::SecondaryWeapon,
                    game_server,
                );
                match (primary_wield_type, secondary_wield_type) {
                    (WieldType::SingleSaber, WieldType::None) => WieldType::SingleSaber,
                    (WieldType::SingleSaber, WieldType::SingleSaber) => WieldType::DualSaber,
                    (WieldType::SinglePistol, WieldType::None) => WieldType::SinglePistol,
                    (WieldType::SinglePistol, WieldType::SinglePistol) => WieldType::DualPistol,
                    (WieldType::None, _) => secondary_wield_type,
                    _ => primary_wield_type,
                }
            })
            .unwrap_or(WieldType::None);
        Character {
            guid: player_guid(guid),
            pos,
            rot,
            scale: 1.0,
            character_type: CharacterType::Player(Box::new(data)),
            state: 0,
            mount_id: None,
            interact_radius: 0.0,
            auto_interact_radius: 0.0,
            instance_guid,
            wield_type: (wield_type, wield_type.holster()),
            holstered: false,
        }
    }

    pub fn chunk(x: f32, z: f32) -> Chunk {
        (
            x.div_euclid(Character::CHUNK_SIZE) as i32,
            z.div_euclid(Character::CHUNK_SIZE) as i32,
        )
    }

    pub fn remove_packets(&self, guid: u64) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: RemoveStandard { guid },
        })?];

        if let Some(mount_id) = self.mount_id {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: RemoveStandard {
                    guid: mount_guid(shorten_player_guid(self.guid)?, mount_id),
                },
            })?);
        }

        Ok(packets)
    }

    pub fn add_packets(
        &self,
        mount_configs: &BTreeMap<u32, MountConfig>,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
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
            CharacterType::Player(_) => {
                let mut packets = Vec::new();
                if let Some(mount_id) = self.mount_id {
                    let short_rider_guid = shorten_player_guid(self.guid)?;
                    let mount_guid = mount_guid(short_rider_guid, mount_id);
                    if let Some(mount_config) = mount_configs.get(&mount_id) {
                        packets.append(&mut spawn_mount_npc(
                            mount_guid,
                            self.guid,
                            mount_config,
                            self.pos,
                            self.rot,
                        )?);
                    } else {
                        println!(
                            "Character {} is mounted on unknown mount ID {}",
                            self.guid, mount_id
                        );
                        return Err(ProcessPacketError::CorruptedPacket);
                    }
                }

                packets
            }
            CharacterType::Fixture(house_guid, fixture) => fixture_packets(
                *house_guid,
                self.guid,
                fixture,
                self.pos,
                self.rot,
                self.scale,
            )?,
        };

        Ok(packets)
    }

    pub fn wield_type(&self) -> WieldType {
        self.wield_type.0
    }

    pub fn brandished_wield_type(&self) -> WieldType {
        if self.holstered {
            self.wield_type.1
        } else {
            self.wield_type.0
        }
    }

    pub fn set_brandished_wield_type(&mut self, wield_type: WieldType) {
        self.wield_type = (wield_type, wield_type.holster());
        self.holstered = false;
    }

    pub fn brandish_or_holster(&mut self) {
        let (old_wield_type, new_wield_type) = self.wield_type;
        self.wield_type = (new_wield_type, old_wield_type);
        self.holstered = !self.holstered;
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
            wield_type: WieldType::None,
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
            scale: character.scale,
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
            wield_type: WieldType::None,
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

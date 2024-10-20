use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

use rand::thread_rng;
use rand_distr::{Distribution, WeightedAliasIndex};
use serde::Deserialize;
use strum::EnumIter;

use crate::{
    game_server::{
        packets::{
            item::{BaseAttachmentGroup, EquipmentSlot, WieldType},
            player_data::EquippedItem,
            player_update::{
                AddNotifications, AddNpc, CustomizationSlot, Icon, NotificationData, NpcRelevance,
                QueueAnimation, RemoveStandard, SingleNotification, SingleNpcRelevance,
                UpdateSpeed,
            },
            tunnel::TunneledPacket,
            ui::ExecuteScriptWithParams,
            update_position::UpdatePlayerPosition,
            GamePacket, Pos,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    teleport_to_zone,
};

use super::{
    guid::{GuidTableIndexer, IndexedGuid},
    housing::fixture_packets,
    inventory::wield_type_from_slot,
    lock_enforcer::{ZoneLockEnforcer, ZoneLockRequest},
    mount::{spawn_mount_npc, MountConfig},
    unique_guid::{mount_guid, npc_guid, player_guid, shorten_player_guid},
    zone::{teleport_within_zone, Zone},
};

pub type WriteLockingBroadcastSupplier = Result<
    Box<dyn FnOnce(&GameServer) -> Result<Vec<Broadcast>, ProcessPacketError>>,
    ProcessPacketError,
>;

pub fn coerce_to_broadcast_supplier(
    f: impl FnOnce(&GameServer) -> Result<Vec<Broadcast>, ProcessPacketError> + 'static,
) -> WriteLockingBroadcastSupplier {
    Ok(Box::new(f))
}

const fn default_scale() -> f32 {
    1.0
}

const fn default_true() -> bool {
    true
}

const fn default_npc_type() -> u32 {
    2
}

const fn default_weight() -> u32 {
    1
}

#[derive(Clone, Deserialize)]
pub struct BaseNpc {
    #[serde(default)]
    pub model_id: u32,
    #[serde(default)]
    pub name_id: u32,
    #[serde(default)]
    pub terrain_object_id: u32,
    #[serde(default = "default_scale")]
    pub scale: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub pos_w: f32,
    #[serde(default)]
    pub rot_x: f32,
    #[serde(default)]
    pub rot_y: f32,
    #[serde(default)]
    pub rot_z: f32,
    #[serde(default)]
    pub rot_w: f32,
    #[serde(default)]
    pub active_animation_slot: i32,
    #[serde(default)]
    pub name_offset_x: f32,
    #[serde(default)]
    pub name_offset_y: f32,
    #[serde(default)]
    pub name_offset_z: f32,
    pub cursor: Option<u8>,
    #[serde(default = "default_true")]
    pub show_name: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub bounce_area_id: i32,
    #[serde(default = "default_npc_type")]
    pub npc_type: u32,
    #[serde(default = "default_true")]
    pub enable_rotation_and_shadow: bool,
}

impl BaseNpc {
    pub fn add_packets(
        &self,
        guid: u64,
        pos: Pos,
        rot: Pos,
        scale: f32,
    ) -> (AddNpc, SingleNpcRelevance) {
        (
            AddNpc {
                guid,
                name_id: self.name_id,
                model_id: self.model_id,
                unknown3: false,
                unknown4: 0,
                unknown5: 0,
                unknown6: 1,
                scale,
                pos,
                rot,
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
                hide_name: !self.show_name,
                name_offset_x: self.name_offset_x,
                name_offset_y: self.name_offset_y,
                name_offset_z: self.name_offset_z,
                terrain_object_id: self.terrain_object_id,
                invisible: !self.visible,
                unknown20: 0.0,
                unknown21: false,
                interactable_size_pct: 100,
                unknown23: -1,
                unknown24: -1,
                active_animation_slot: self.active_animation_slot,
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
                disable_rotation_and_shadow: !self.enable_rotation_and_shadow,
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
                bounce_area_id: self.bounce_area_id,
                unknown42: 0,
                collision: true,
                unknown44: 0,
                npc_type: self.npc_type,
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
                pet_customization_model_name1: "".to_string(),
                pet_customization_model_name2: "".to_string(),
                pet_customization_model_name3: "".to_string(),
                override_terrain_model: false,
                hover_glow: 0,
                hover_description: 0,
                fly_over_effect: 0,
                unknown65: 0,
                unknown66: 0,
                unknown67: 0,
                disable_move_to_interact: false,
                unknown69: 0.0,
                unknown70: 0.0,
                unknown71: 0,
                icon_id: Icon::None,
            },
            SingleNpcRelevance {
                guid,
                cursor: self.cursor,
                unknown1: false,
            },
        )
    }
}

#[derive(Clone, Deserialize)]
pub struct TickableNpcState {
    #[serde(default = "default_weight")]
    pub weight: u32,
    pub speed: f32,
    pub new_pos_x: Option<f32>,
    pub new_pos_y: Option<f32>,
    pub new_pos_z: Option<f32>,
    #[serde(default)]
    pub new_rot_x: f32,
    #[serde(default)]
    pub new_rot_y: f32,
    #[serde(default)]
    pub new_rot_z: f32,
    #[serde(default)]
    pub new_pos_offset_x: f32,
    #[serde(default)]
    pub new_pos_offset_y: f32,
    #[serde(default)]
    pub new_pos_offset_z: f32,
    pub animation_id: Option<u32>,
    pub duration_millis: u64,
}

impl TickableNpcState {
    pub fn new_pos(&self, current_pos: Pos) -> Pos {
        Pos {
            x: self.new_pos_x.unwrap_or(current_pos.x) + self.new_pos_offset_x,
            y: self.new_pos_y.unwrap_or(current_pos.y) + self.new_pos_offset_y,
            z: self.new_pos_z.unwrap_or(current_pos.z) + self.new_pos_offset_z,
            w: current_pos.w,
        }
    }

    pub fn to_packets(
        &self,
        guid: u64,
        current_pos: Pos,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let mut packets = Vec::new();
        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdateSpeed {
                guid,
                speed: self.speed,
            },
        })?);

        let new_pos = self.new_pos(current_pos);
        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdatePlayerPosition {
                guid,
                pos_x: new_pos.x,
                pos_y: new_pos.y,
                pos_z: new_pos.z,
                rot_x: self.new_rot_x,
                rot_y: self.new_rot_y,
                rot_z: self.new_rot_z,
                stop_at_destination: true,
                unknown: 0,
            },
        })?);

        if let Some(animation_id) = self.animation_id {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: QueueAnimation {
                    character_guid: guid,
                    animation_id,
                    queue_pos: 1,
                    delay_seconds: 0.0,
                    duration_seconds: Duration::from_millis(self.duration_millis).as_secs_f32(),
                },
            })?);
        }

        Ok(packets)
    }
}

#[derive(Clone, Default, Deserialize, Eq, PartialEq)]
pub enum TickableNpcStateOrder {
    #[default]
    Sequential,
    WeightedRandom,
}

#[derive(Clone, Deserialize)]
pub struct TickableNpcStateTracker {
    #[serde(default)]
    states: Vec<TickableNpcState>,
    #[serde(default)]
    state_order: TickableNpcStateOrder,
    #[serde(skip_deserializing)]
    current_state: Option<usize>,
    #[serde(skip_deserializing, default = "Instant::now")]
    last_state_change: Instant,
    #[serde(skip_deserializing)]
    distribution: Option<WeightedAliasIndex<u32>>,
}

impl TickableNpcStateTracker {
    pub fn tick(
        &mut self,
        guid: u64,
        current_pos: Pos,
        now: Instant,
    ) -> Result<(Vec<Vec<u8>>, Pos), ProcessPacketError> {
        if self.states.is_empty() {
            return Ok((Vec::new(), current_pos));
        }

        let time_since_last_change = now.saturating_duration_since(self.last_state_change);

        let current_state_index = self.current_state.unwrap_or(self.states.len() - 1);
        let current_state = &self.states[current_state_index];
        if time_since_last_change > Duration::from_millis(current_state.duration_millis) {
            let new_state_index = if self.state_order == TickableNpcStateOrder::Sequential {
                current_state_index.saturating_add(1) % self.states.len()
            } else {
                if self.distribution.is_none() {
                    let weights = self.states.iter().map(|state| state.weight).collect();
                    let new_distribution = WeightedAliasIndex::new(weights)
                        .expect("Couldn't create weighted alias index");
                    self.distribution = Some(new_distribution);
                }

                let weighted_alias_index = self
                    .distribution
                    .as_ref()
                    .expect("Distribution should have already been created");
                weighted_alias_index.sample(&mut thread_rng())
            };
            self.current_state = Some(new_state_index);
            self.last_state_change = Instant::now();

            let new_state = &self.states[new_state_index];
            Ok((
                new_state.to_packets(guid, current_pos)?,
                new_state.new_pos(current_pos),
            ))
        } else {
            Ok((Vec::new(), current_pos))
        }
    }

    pub fn tickable(&self) -> bool {
        !self.states.is_empty()
    }
}

#[derive(Clone, Deserialize)]
pub struct AmbientNpc {
    #[serde(flatten)]
    pub base_npc: BaseNpc,
    #[serde(flatten)]
    pub state_tracker: TickableNpcStateTracker,
}

impl AmbientNpc {
    pub fn add_packets(
        &self,
        guid: u64,
        pos: Pos,
        rot: Pos,
        scale: f32,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let (add_npc, enable_interaction) = self.base_npc.add_packets(guid, pos, rot, scale);
        let packets = vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: add_npc,
            })?,
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NpcRelevance {
                    new_states: vec![enable_interaction],
                },
            })?,
        ];

        Ok(packets)
    }

    pub fn tick(
        &mut self,
        guid: u64,
        current_pos: Pos,
        now: Instant,
    ) -> Result<(Vec<Vec<u8>>, Pos), ProcessPacketError> {
        self.state_tracker.tick(guid, current_pos, now)
    }

    pub fn tickable(&self) -> bool {
        self.state_tracker.tickable()
    }
}

#[derive(Clone, Deserialize)]
pub struct Door {
    #[serde(flatten)]
    pub base_npc: BaseNpc,
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

impl Door {
    pub fn add_packets(
        &self,
        guid: u64,
        pos: Pos,
        rot: Pos,
        scale: f32,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let (mut add_npc, mut enable_interaction) =
            self.base_npc.add_packets(guid, pos, rot, scale);
        add_npc.disable_interact_popup = true;
        enable_interaction.cursor = enable_interaction.cursor.or(Some(55));
        let packets = vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: add_npc,
            })?,
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NpcRelevance {
                    new_states: vec![enable_interaction],
                },
            })?,
        ];

        Ok(packets)
    }

    pub fn interact(
        &self,
        requester: u32,
        source_zone_guid: u64,
        zones_lock_enforcer: &ZoneLockEnforcer,
    ) -> WriteLockingBroadcastSupplier {
        let destination_pos = Pos {
            x: self.destination_pos_x,
            y: self.destination_pos_y,
            z: self.destination_pos_z,
            w: self.destination_pos_w,
        };
        let destination_rot = Pos {
            x: self.destination_rot_x,
            y: self.destination_rot_y,
            z: self.destination_rot_z,
            w: self.destination_rot_w,
        };

        let destination_zone_guid = if let &Some(destination_zone_guid) = &self.destination_zone {
            destination_zone_guid
        } else if let &Some(destination_zone_template) = &self.destination_zone_template {
            zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                read_guids: Vec::new(),
                write_guids: Vec::new(),
                zone_consumer: |zones_table_read_handle, _, _| {
                    GameServer::any_instance(zones_table_read_handle, destination_zone_template)
                },
            })?
        } else {
            source_zone_guid
        };

        coerce_to_broadcast_supplier(move |game_server| {
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
}

#[derive(Clone, Deserialize)]
pub struct Transport {
    #[serde(flatten)]
    pub base_npc: BaseNpc,
    pub show_icon: bool,
    pub large_icon: bool,
    pub show_hover_description: bool,
}

impl Transport {
    pub fn add_packets(
        &self,
        guid: u64,
        pos: Pos,
        rot: Pos,
        scale: f32,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let (mut add_npc, enable_interaction) = self.base_npc.add_packets(guid, pos, rot, scale);
        add_npc.hover_description = if self.show_hover_description {
            self.base_npc.name_id
        } else {
            0
        };
        let packets = vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: add_npc,
            })?,
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NpcRelevance {
                    new_states: vec![enable_interaction],
                },
            })?,
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AddNotifications {
                    notifications: vec![SingleNotification {
                        guid,
                        unknown1: 0,
                        notification: Some(NotificationData {
                            unknown1: 0,
                            icon_id: if self.large_icon { 46 } else { 37 },
                            unknown3: 0,
                            name_id: 0,
                            unknown4: 0,
                            hide_icon: !self.show_icon,
                            unknown6: 0,
                        }),
                        unknown2: false,
                    }],
                },
            })?,
        ];

        Ok(packets)
    }

    pub fn interact(&self, requester: u32) -> WriteLockingBroadcastSupplier {
        coerce_to_broadcast_supplier(move |_| {
            Ok(vec![Broadcast::Single(
                requester,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: false,
                    inner: ExecuteScriptWithParams {
                        script_name: "UIGlobal.ShowGalaxyMap".to_string(),
                        params: vec![],
                    },
                })?],
            )])
        })
    }
}

#[derive(Clone)]
pub struct BattleClass {
    pub items: BTreeMap<EquipmentSlot, EquippedItem>,
}

#[derive(Clone)]
pub struct Player {
    pub ready: bool,
    pub member: bool,
    pub credits: u32,
    pub battle_classes: BTreeMap<u32, BattleClass>,
    pub active_battle_class: u32,
    pub inventory: BTreeSet<u32>,
    pub customizations: BTreeMap<CustomizationSlot, u32>,
}

impl Player {
    pub fn add_packets(
        &self,
        guid: u64,
        mount_id: Option<u32>,
        pos: Pos,
        rot: Pos,
        mount_configs: &BTreeMap<u32, MountConfig>,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let mut packets = Vec::new();
        if let Some(mount_id) = mount_id {
            let short_rider_guid = shorten_player_guid(guid)?;
            let mount_guid = mount_guid(short_rider_guid, mount_id);
            if let Some(mount_config) = mount_configs.get(&mount_id) {
                packets.append(&mut spawn_mount_npc(
                    mount_guid,
                    guid,
                    mount_config,
                    pos,
                    rot,
                )?);
            } else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Character {} is mounted on unknown mount ID {}",
                        guid, mount_id
                    ),
                ));
            }
        }

        Ok(packets)
    }
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
    AmbientNpc(AmbientNpc),
    Door(Door),
    Transport(Transport),
    Player(Box<Player>),
    Fixture(u64, CurrentFixture),
}

#[derive(Copy, Clone, Eq, EnumIter, PartialOrd, PartialEq, Ord)]
pub enum CharacterCategory {
    PlayerReady,
    PlayerUnready,
    NpcAutoInteractEnabled,
    NpcTickable,
    NpcBasic,
}

#[derive(Clone)]
pub struct NpcTemplate {
    pub discriminant: u8,
    pub index: u16,
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
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
pub type CharacterIndex = (CharacterCategory, u64, Chunk);

#[derive(Clone)]
pub struct Character {
    pub guid: u64,
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
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
            match &self.character_type {
                CharacterType::Player(player) => match player.ready {
                    true => CharacterCategory::PlayerReady,
                    false => CharacterCategory::PlayerUnready,
                },
                _ => match self.auto_interact_radius > 0.0 {
                    true => CharacterCategory::NpcAutoInteractEnabled,
                    false => match self.tickable() {
                        true => CharacterCategory::NpcTickable,
                        false => CharacterCategory::NpcBasic,
                    },
                },
            },
            self.instance_guid,
            Character::chunk(self.pos.x, self.pos.z),
        )
    }
}

impl Character {
    pub const MIN_CHUNK: (i32, i32) = (i32::MIN, i32::MIN);
    pub const MAX_CHUNK: (i32, i32) = (i32::MAX, i32::MAX);
    const CHUNK_SIZE: f32 = 200.0;

    pub fn new(
        guid: u64,
        pos: Pos,
        rot: Pos,
        scale: f32,
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

    pub fn remove_packets(&self) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: RemoveStandard { guid: self.guid },
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
            CharacterType::AmbientNpc(ambient_npc) => {
                ambient_npc.add_packets(self.guid, self.pos, self.rot, self.scale)?
            }
            CharacterType::Door(door) => {
                door.add_packets(self.guid, self.pos, self.rot, self.scale)?
            }
            CharacterType::Transport(transport) => {
                transport.add_packets(self.guid, self.pos, self.rot, self.scale)?
            }
            CharacterType::Player(player) => {
                player.add_packets(self.guid, self.mount_id, self.pos, self.rot, mount_configs)?
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

    pub fn tick<'a>(
        &mut self,
        now: Instant,
        characters_table_handle: &'a impl GuidTableIndexer<'a, u64, Character, CharacterIndex>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let (_, _, chunk) = self.index();
        let everyone =
            Zone::all_players_nearby(None, chunk, self.instance_guid, characters_table_handle)?;
        match &mut self.character_type {
            CharacterType::AmbientNpc(ambient_npc) => {
                let (packets, new_pos) = ambient_npc.tick(self.guid, self.pos, now)?;
                self.pos = new_pos;
                Ok(vec![Broadcast::Multi(everyone, packets)])
            }
            _ => Ok(Vec::new()),
        }
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

    pub fn interact(
        &self,
        requester: u32,
        source_zone_guid: u64,
        zones_lock_enforcer: &ZoneLockEnforcer,
    ) -> WriteLockingBroadcastSupplier {
        match &self.character_type {
            CharacterType::Door(door) => {
                door.interact(requester, source_zone_guid, zones_lock_enforcer)
            }
            CharacterType::Transport(transport) => transport.interact(requester),
            _ => coerce_to_broadcast_supplier(|_| Ok(Vec::new())),
        }
    }

    fn tickable(&self) -> bool {
        match &self.character_type {
            CharacterType::AmbientNpc(ambient_npc) => ambient_npc.tickable(),
            _ => false,
        }
    }
}

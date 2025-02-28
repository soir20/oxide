use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::{Duration, Instant},
};

use rand::thread_rng;
use rand_distr::{Distribution, WeightedAliasIndex};
use serde::Deserialize;
use strum::EnumIter;

use crate::{
    game_server::{
        packets::{
            chat::{ActionBarTextColor, SendStringId},
            command::PlaySoundIdOnTarget,
            item::{Attachment, BaseAttachmentGroup, EquipmentSlot, ItemDefinition, WieldType},
            minigame::ScoreEntry,
            player_data::EquippedItem,
            player_update::{
                AddNotifications, AddNpc, AddPc, Customization, CustomizationSlot, Hostility, Icon,
                NameplateImage, NotificationData, NpcRelevance, QueueAnimation, RemoveStandard,
                SetAnimation, SingleNotification, SingleNpcRelevance, UpdateSpeed,
            },
            tunnel::TunneledPacket,
            ui::ExecuteScriptWithParams,
            update_position::UpdatePlayerPosition,
            GamePacket, GuidTarget, Name, Pos, Rgba, Target,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    teleport_to_zone,
};

use super::{
    distance3_pos,
    guid::{Guid, IndexedGuid},
    housing::fixture_packets,
    inventory::wield_type_from_slot,
    lock_enforcer::CharacterReadGuard,
    minigame::PlayerMinigameStats,
    mount::{spawn_mount_npc, MountConfig},
    unique_guid::{mount_guid, npc_guid, player_guid, shorten_player_guid},
    zone::teleport_within_zone,
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

pub const CHAT_BUBBLE_VISIBLE_RADIUS: f32 = 32.0;

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

#[derive(Clone, Default, Deserialize, Eq, PartialEq)]
pub enum CursorUpdate {
    #[default]
    Keep,
    Disable,
    Enable {
        new_cursor: u8,
    },
}

#[derive(Clone, Deserialize)]
pub struct BaseNpcConfig {
    pub key: Option<String>,
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
    pub enable_interact_popup: bool,
    #[serde(default = "default_true")]
    pub show_name: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub bounce_area_id: i32,
    #[serde(default = "default_npc_type")]
    pub npc_type: u32,
    #[serde(default)]
    pub enable_tilt: bool,
    #[serde(default)]
    pub tickable_procedures: HashMap<String, TickableProcedureConfig>,
    #[serde(default)]
    pub first_possible_procedures: Vec<String>,
    pub synchronize_with: Option<String>,
}

#[derive(Clone)]
pub struct BaseNpc {
    pub model_id: u32,
    pub name_id: u32,
    pub terrain_object_id: u32,
    pub name_offset_x: f32,
    pub name_offset_y: f32,
    pub name_offset_z: f32,
    pub enable_interact_popup: bool,
    pub show_name: bool,
    pub visible: bool,
    pub bounce_area_id: i32,
    pub npc_type: u32,
    pub enable_tilt: bool,
}

impl BaseNpc {
    pub fn add_packets(&self, character: &Character) -> (AddNpc, SingleNpcRelevance) {
        (
            AddNpc {
                guid: character.guid(),
                name_id: self.name_id,
                model_id: self.model_id,
                unknown3: true,
                chat_text_color: Character::DEFAULT_CHAT_TEXT_COLOR,
                chat_bubble_color: Character::DEFAULT_CHAT_BUBBLE_COLOR,
                chat_scale: 1,
                scale: character.stats.scale,
                pos: character.stats.pos,
                rot: character.stats.rot,
                spawn_animation_id: -1,
                attachments: vec![],
                hostility: Hostility::Neutral,
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
                speed: character.stats.speed.total(),
                unknown21: false,
                interactable_size_pct: 100,
                unknown23: -1,
                unknown24: -1,
                looping_animation_id: character.stats.animation_id,
                unknown26: false,
                ignore_position: false,
                sub_title_id: 0,
                one_shot_animation_id: -1,
                temporary_appearance: 0,
                effects: vec![],
                disable_interact_popup: !self.enable_interact_popup,
                unknown33: 0,
                unknown34: false,
                show_health: false,
                hide_despawn_fade: false,
                enable_tilt: self.enable_tilt,
                base_attachment_group: BaseAttachmentGroup {
                    unknown1: 0,
                    unknown2: "".to_string(),
                    unknown3: "".to_string(),
                    unknown4: 0,
                    unknown5: "".to_string(),
                },
                tilt: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                unknown40: 0,
                bounce_area_id: self.bounce_area_id,
                image_set_id: 0,
                collision: true,
                rider_guid: 0,
                npc_type: self.npc_type,
                interact_popup_radius: character.stats.interact_radius,
                target: Target::default(),
                variables: vec![],
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
                guid: character.guid(),
                cursor: character.stats.cursor,
                unknown1: false,
            },
        )
    }
}

impl From<BaseNpcConfig> for BaseNpc {
    fn from(value: BaseNpcConfig) -> Self {
        BaseNpc {
            model_id: value.model_id,
            name_id: value.name_id,
            terrain_object_id: value.terrain_object_id,
            name_offset_x: value.name_offset_x,
            name_offset_y: value.name_offset_y,
            name_offset_z: value.name_offset_z,
            enable_interact_popup: value.enable_interact_popup,
            show_name: value.show_name,
            visible: value.visible,
            bounce_area_id: value.bounce_area_id,
            npc_type: value.npc_type,
            enable_tilt: value.enable_tilt,
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct TickableStep {
    pub speed: Option<f32>,
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
    pub animation_id: Option<i32>,
    pub one_shot_animation_id: Option<i32>,
    pub chat_message_id: Option<u32>,
    pub sound_id: Option<u32>,
    #[serde(default)]
    pub cursor: CursorUpdate,
    pub duration_millis: u64,
}

impl TickableStep {
    pub fn new_pos(&self, current_pos: Pos) -> Pos {
        Pos {
            x: self.new_pos_x.unwrap_or(current_pos.x) + self.new_pos_offset_x,
            y: self.new_pos_y.unwrap_or(current_pos.y) + self.new_pos_offset_y,
            z: self.new_pos_z.unwrap_or(current_pos.z) + self.new_pos_offset_z,
            w: current_pos.w,
        }
    }

    pub fn apply(
        &self,
        character: &mut CharacterStats,
        nearby_player_guids: &[u32],
        nearby_players: &BTreeMap<u64, CharacterReadGuard>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let mut packets_for_all = Vec::new();

        if let Some(speed) = self.speed {
            character.speed.base = speed;
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: UpdateSpeed {
                    guid: Guid::guid(character),
                    speed,
                },
            })?);
        }

        let new_pos = self.new_pos(character.pos);
        character.pos = new_pos;
        packets_for_all.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdatePlayerPosition {
                guid: Guid::guid(character),
                pos_x: new_pos.x,
                pos_y: new_pos.y,
                pos_z: new_pos.z,
                rot_x: self.new_rot_x,
                rot_y: self.new_rot_y,
                rot_z: self.new_rot_z,
                character_state: 1,
                unknown: 0,
            },
        })?);

        if let Some(animation_id) = self.animation_id {
            character.animation_id = animation_id;
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SetAnimation {
                    character_guid: Guid::guid(character),
                    animation_id,
                    animation_group_id: -1,
                    override_animation: true,
                },
            })?);
        }

        if let Some(animation_id) = self.one_shot_animation_id {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: QueueAnimation {
                    character_guid: Guid::guid(character),
                    animation_id,
                    queue_pos: 0,
                    delay_seconds: 0.0,
                    duration_seconds: self.duration_millis as f32 / 1000.0,
                },
            })?);
        }

        if let Some(sound_id) = self.sound_id {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: PlaySoundIdOnTarget {
                    sound_id,
                    target: Target::Guid(GuidTarget {
                        fallback_pos: character.pos,
                        guid: Guid::guid(character),
                    }),
                },
            })?);
        }

        if self.cursor != CursorUpdate::Keep {
            let cursor = if let CursorUpdate::Enable { new_cursor } = self.cursor {
                Some(new_cursor)
            } else {
                None
            };

            character.cursor = cursor;

            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NpcRelevance {
                    new_states: vec![SingleNpcRelevance {
                        guid: Guid::guid(character),
                        cursor,
                        unknown1: false,
                    }],
                },
            })?);
        }

        let mut broadcasts = vec![Broadcast::Multi(
            nearby_player_guids.to_vec(),
            packets_for_all,
        )];

        if let Some(chat_message_id) = self.chat_message_id {
            let chat_packets = vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SendStringId {
                    sender_guid: Guid::guid(character),
                    message_id: chat_message_id,
                    is_anonymous: false,
                    unknown2: false,
                    is_action_bar_message: false,
                    action_bar_text_color: ActionBarTextColor::default(),
                    target_guid: 0,
                    owner_guid: 0,
                    unknown7: 0,
                },
            })?];

            let recipients = nearby_player_guids
                .iter()
                .filter(|guid| {
                    let pos = distance3_pos(
                        nearby_players[&player_guid(**guid)].stats.pos,
                        character.pos,
                    );
                    pos <= CHAT_BUBBLE_VISIBLE_RADIUS
                })
                .cloned()
                .collect();

            broadcasts.push(Broadcast::Multi(recipients, chat_packets));
        }

        Ok(broadcasts)
    }
}

#[derive(Clone, Deserialize)]
pub struct TickableProcedureConfig {
    #[serde(default = "default_weight")]
    pub weight: u32,
    pub steps: Vec<TickableStep>,
    #[serde(default)]
    pub next_possible_procedures: Vec<String>,
}

pub enum TickResult {
    TickedCurrentProcedure(Result<Vec<Broadcast>, ProcessPacketError>),
    MustChangeProcedure(String),
}

#[derive(Clone)]
pub struct TickableProcedure {
    steps: Vec<TickableStep>,
    current_step: Option<(usize, Instant)>,
    distribution: WeightedAliasIndex<u32>,
    next_possible_procedures: Vec<String>,
}

impl TickableProcedure {
    pub fn from_config(
        config: TickableProcedureConfig,
        all_procedures: &HashMap<String, TickableProcedureConfig>,
    ) -> Self {
        let (distribution, next_possible_procedures) = if config.next_possible_procedures.is_empty()
        {
            (
                WeightedAliasIndex::new(
                    all_procedures
                        .values()
                        .map(|procedure| procedure.weight)
                        .collect(),
                ),
                all_procedures.keys().cloned().collect(),
            )
        } else {
            let weights = config
                .next_possible_procedures
                .iter()
                .map(|procedure_key| {
                    if let Some(procedure) = all_procedures.get(procedure_key) {
                        procedure.weight
                    } else {
                        panic!("Reference to unknown procedure {}", procedure_key)
                    }
                })
                .collect();
            (
                WeightedAliasIndex::new(weights),
                config.next_possible_procedures,
            )
        };

        TickableProcedure {
            steps: config.steps,
            current_step: None,
            distribution: distribution.expect("Couldn't create weighted alias index"),
            next_possible_procedures,
        }
    }

    pub fn tick(
        &mut self,
        character: &mut CharacterStats,
        now: Instant,
        nearby_player_guids: &[u32],
        nearby_players: &BTreeMap<u64, CharacterReadGuard>,
    ) -> TickResult {
        self.panic_if_empty();

        let should_change_steps =
            if let Some((current_step_index, last_step_change)) = self.current_step {
                let time_since_last_step_change = now.saturating_duration_since(last_step_change);
                let current_step = &self.steps[current_step_index];

                time_since_last_step_change >= Duration::from_millis(current_step.duration_millis)
            } else {
                true
            };

        if should_change_steps {
            let new_step_index = self
                .current_step
                .map(|(current_step_index, _)| current_step_index.saturating_add(1))
                .unwrap_or_default();
            if new_step_index >= self.steps.len() {
                TickResult::MustChangeProcedure(self.next_procedure())
            } else {
                self.current_step = Some((new_step_index, now));
                TickResult::TickedCurrentProcedure(self.steps[new_step_index].apply(
                    character,
                    nearby_player_guids,
                    nearby_players,
                ))
            }
        } else {
            TickResult::TickedCurrentProcedure(Ok(Vec::new()))
        }
    }

    fn next_procedure(&mut self) -> String {
        let next_procedure_index = self.distribution.sample(&mut thread_rng());
        self.next_possible_procedures[next_procedure_index].clone()
    }

    pub fn reset(&mut self) {
        self.current_step = None;
    }

    fn panic_if_empty(&self) {
        if self.steps.is_empty() {
            panic!("Every tickable NPC procedure must have steps");
        }
    }
}

#[derive(Clone)]
pub struct TickableProcedureTracker {
    procedures: HashMap<String, TickableProcedure>,
    current_procedure_key: String,
    last_procedure_change: Instant,
}

impl TickableProcedureTracker {
    pub fn new(
        procedures: HashMap<String, TickableProcedureConfig>,
        first_possible_procedures: Vec<String>,
    ) -> Self {
        let current_procedure_key = if procedures.is_empty() {
            String::from("")
        } else {
            let (weights, procedure_keys): (Vec<u32>, Vec<&String>) =
                if first_possible_procedures.is_empty() {
                    let weights = procedures
                        .values()
                        .map(|procedure| procedure.weight)
                        .collect();
                    (weights, procedures.keys().collect())
                } else {
                    let weights = first_possible_procedures
                        .iter()
                        .map(|procedure_key| {
                            if let Some(procedure) = procedures.get(procedure_key) {
                                procedure.weight
                            } else {
                                panic!("Reference to unknown procedure {}", procedure_key);
                            }
                        })
                        .collect();
                    (weights, first_possible_procedures.iter().collect())
                };

            let distribution =
                WeightedAliasIndex::new(weights).expect("Couldn't create weighted alias index");
            let index = distribution.sample(&mut thread_rng());

            procedure_keys[index].clone()
        };

        TickableProcedureTracker {
            current_procedure_key,
            procedures: procedures
                .iter()
                .map(|(key, config)| {
                    (
                        key.clone(),
                        TickableProcedure::from_config(config.clone(), &procedures),
                    )
                })
                .collect(),
            last_procedure_change: Instant::now(),
        }
    }

    pub fn current_tickable_procedure(&self) -> Option<&String> {
        if self.procedures.is_empty() {
            None
        } else {
            Some(&self.current_procedure_key)
        }
    }

    pub fn last_procedure_change(&self) -> Instant {
        self.last_procedure_change
    }

    pub fn set_procedure_if_exists(&mut self, new_procedure_key: String, now: Instant) {
        if self.procedures.contains_key(&new_procedure_key) {
            let current_procedure = self
                .procedures
                .get_mut(&self.current_procedure_key)
                .expect("Missing procedure");
            current_procedure.reset();

            self.current_procedure_key = new_procedure_key;
            self.last_procedure_change = now;
        }
    }

    pub fn tick(
        &mut self,
        character: &mut CharacterStats,
        now: Instant,
        nearby_player_guids: &[u32],
        nearby_players: &BTreeMap<u64, CharacterReadGuard>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if self.procedures.is_empty() {
            return Ok(Vec::new());
        }

        let mut current_procedure = self
            .procedures
            .get_mut(&self.current_procedure_key)
            .expect("Missing procedure");
        loop {
            let tick_result =
                current_procedure.tick(character, now, nearby_player_guids, nearby_players);
            if let TickResult::TickedCurrentProcedure(result) = tick_result {
                break result;
            } else if let TickResult::MustChangeProcedure(procedure_key) = tick_result {
                current_procedure.reset();
                self.current_procedure_key = procedure_key;
                current_procedure = self
                    .procedures
                    .get_mut(&self.current_procedure_key)
                    .expect("Missing procedure");
                self.last_procedure_change = now;
            }
        }
    }

    pub fn tickable(&self) -> bool {
        !self.procedures.is_empty()
    }
}

#[derive(Clone, Deserialize)]
pub struct AmbientNpcConfig {
    #[serde(flatten)]
    pub base_npc: BaseNpcConfig,
    pub procedure_on_interact: Option<String>,
}

#[derive(Clone)]
pub struct AmbientNpc {
    pub base_npc: BaseNpc,
    pub procedure_on_interact: Option<String>,
}

impl AmbientNpc {
    pub fn add_packets(&self, character: &Character) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let (add_npc, enable_interaction) = self.base_npc.add_packets(character);
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

    pub fn interact(&self, character: &Character) -> Option<String> {
        if let Some(new_procedure) = &self.procedure_on_interact {
            let is_different_procedure = character
                .current_tickable_procedure()
                .map(|current_procedure| current_procedure != new_procedure)
                .unwrap_or(true);
            if is_different_procedure {
                return Some(new_procedure.clone());
            }
        }

        None
    }
}

impl From<AmbientNpcConfig> for AmbientNpc {
    fn from(value: AmbientNpcConfig) -> Self {
        AmbientNpc {
            base_npc: value.base_npc.into(),
            procedure_on_interact: value.procedure_on_interact,
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct DoorConfig {
    #[serde(flatten)]
    pub base_npc: BaseNpcConfig,
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
    #[serde(default = "default_true")]
    pub update_previous_location_on_leave: bool,
}

#[derive(Clone)]
pub struct Door {
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
    pub update_previous_location_on_leave: bool,
}

impl Door {
    pub fn add_packets(&self, character: &Character) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let (mut add_npc, mut enable_interaction) = self.base_npc.add_packets(character);
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

    pub fn interact(&self, requester: u32, source_zone_guid: u64) -> WriteLockingBroadcastSupplier {
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

        let destination_zone = self.destination_zone;
        let destination_zone_template = self.destination_zone_template;

        coerce_to_broadcast_supplier(move |game_server| {
            game_server.lock_enforcer().write_characters(
                |characters_table_write_handle, zones_lock_enforcer| {
                    zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                        let destination_zone_guid = if let &Some(destination_zone_guid) =
                            &destination_zone
                        {
                            destination_zone_guid
                        } else if let &Some(destination_zone_template) = &destination_zone_template
                        {
                            game_server.get_or_create_instance(
                                characters_table_write_handle,
                                zones_table_write_handle,
                                destination_zone_template,
                                1,
                            )?
                        } else {
                            source_zone_guid
                        };

                        if source_zone_guid != destination_zone_guid {
                            if let Some(destination_lock) =
                                zones_table_write_handle.get(destination_zone_guid)
                            {
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
                                Ok(Vec::new())
                            }
                        } else {
                            teleport_within_zone(requester, destination_pos, destination_rot)
                        }
                    })
                },
            )
        })
    }
}

impl From<DoorConfig> for Door {
    fn from(value: DoorConfig) -> Self {
        Door {
            base_npc: value.base_npc.into(),
            destination_pos_x: value.destination_pos_x,
            destination_pos_y: value.destination_pos_y,
            destination_pos_z: value.destination_pos_z,
            destination_pos_w: value.destination_pos_w,
            destination_rot_x: value.destination_rot_x,
            destination_rot_y: value.destination_rot_y,
            destination_rot_z: value.destination_rot_z,
            destination_rot_w: value.destination_rot_w,
            destination_zone_template: value.destination_zone_template,
            destination_zone: value.destination_zone,
            update_previous_location_on_leave: value.update_previous_location_on_leave,
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct TransportConfig {
    #[serde(flatten)]
    pub base_npc: BaseNpcConfig,
    pub show_icon: bool,
    pub large_icon: bool,
    pub show_hover_description: bool,
}

#[derive(Clone)]
pub struct Transport {
    pub base_npc: BaseNpc,
    pub show_icon: bool,
    pub large_icon: bool,
    pub show_hover_description: bool,
}

impl Transport {
    pub fn add_packets(&self, character: &Character) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let (mut add_npc, enable_interaction) = self.base_npc.add_packets(character);
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
                        guid: character.guid(),
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

impl From<TransportConfig> for Transport {
    fn from(value: TransportConfig) -> Self {
        Transport {
            base_npc: value.base_npc.into(),
            show_icon: value.show_icon,
            large_icon: value.large_icon,
            show_hover_description: value.show_hover_description,
        }
    }
}

#[derive(Clone)]
pub struct BattleClass {
    pub items: BTreeMap<EquipmentSlot, EquippedItem>,
}

#[derive(Clone)]
pub struct MinigameStatus {
    pub stage_group_guid: i32,
    pub stage_guid: i32,
    pub game_created: bool,
    pub game_won: bool,
    pub score_entries: Vec<ScoreEntry>,
    pub total_score: i32,
    pub start_time: Instant,
}

#[derive(Clone)]
pub struct OwnedMatchmakingGroup {
    pub stage_group_guid: i32,
    pub stage_guid: i32,
    pub since: Instant,
}

#[derive(Clone)]
pub struct PreviousLocation {
    pub template_guid: u8,
    pub pos: Pos,
    pub rot: Pos,
}

#[derive(Clone)]
pub struct Player {
    pub first_load: bool,
    pub ready: bool,
    pub name: Name,
    pub squad_guid: Option<u64>,
    pub member: bool,
    pub credits: u32,
    pub battle_classes: BTreeMap<u32, BattleClass>,
    pub active_battle_class: u32,
    pub inventory: BTreeSet<u32>,
    pub customizations: BTreeMap<CustomizationSlot, u32>,
    pub minigame_stats: PlayerMinigameStats,
    pub matchmaking_group: Option<CharacterMatchmakingGroupIndex>,
    pub owned_matchmaking_group: Option<OwnedMatchmakingGroup>,
    pub minigame_status: Option<MinigameStatus>,
    pub update_previous_location_on_leave: bool,
    pub previous_location: PreviousLocation,
}

impl Player {
    pub fn add_packets(
        &self,
        character: &Character,
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        if !self.ready {
            return Ok(Vec::new());
        }

        let mut mount_packets = Vec::new();
        let mut player_mount_guid = 0;
        if let Some(mount_id) = character.stats.mount_id {
            let short_rider_guid = shorten_player_guid(character.guid())?;
            player_mount_guid = mount_guid(short_rider_guid, mount_id);
            if let Some(mount_config) = mount_configs.get(&mount_id) {
                mount_packets.append(&mut spawn_mount_npc(
                    player_mount_guid,
                    character.guid(),
                    mount_config,
                    character.stats.pos,
                    character.stats.rot,
                )?);
            } else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Character {} is mounted on unknown mount ID {}",
                        character.guid(),
                        mount_id
                    ),
                ));
            }
        }

        let attachments: Vec<Attachment> = self
            .battle_classes
            .get(&self.active_battle_class)
            .map(|battle_class| {
                battle_class
                    .items
                    .iter()
                    .filter_map(|(slot, item)| {
                        let tint_override = match slot {
                            EquipmentSlot::PrimarySaberShape => battle_class
                                .items
                                .get(&EquipmentSlot::PrimarySaberColor)
                                .and_then(|item| item_definitions.get(&item.guid))
                                .map(|item_def| item_def.tint),
                            EquipmentSlot::SecondarySaberShape => battle_class
                                .items
                                .get(&EquipmentSlot::SecondarySaberColor)
                                .and_then(|item| item_definitions.get(&item.guid))
                                .map(|item_def| item_def.tint),
                            _ => None,
                        };

                        item_definitions
                            .get(&item.guid)
                            .map(|item_definition| Attachment {
                                model_name: item_definition.model_name.clone(),
                                texture_alias: item_definition.texture_alias.clone(),
                                tint_alias: item_definition.tint_alias.clone(),
                                tint: tint_override.unwrap_or(item_definition.tint),
                                composite_effect: item_definition.composite_effect,
                                slot: *slot,
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AddPc {
                guid: character.guid(),
                name: self.name.clone(),
                body_model: self
                    .customizations
                    .get(&CustomizationSlot::BodyModel)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param2)
                    .unwrap_or_default(),
                chat_text_color: Character::DEFAULT_CHAT_TEXT_COLOR,
                chat_bubble_color: Character::DEFAULT_CHAT_BUBBLE_COLOR,
                chat_scale: 1,
                pos: character.stats.pos,
                rot: character.stats.rot,
                attachments,
                head_model: self
                    .customizations
                    .get(&CustomizationSlot::HeadModel)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param1.clone())
                    .unwrap_or_default(),
                hair_model: self
                    .customizations
                    .get(&CustomizationSlot::HairStyle)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param1.clone())
                    .unwrap_or_default(),
                hair_color: self
                    .customizations
                    .get(&CustomizationSlot::HairColor)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param2)
                    .unwrap_or_default(),
                eye_color: self
                    .customizations
                    .get(&CustomizationSlot::EyeColor)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param2)
                    .unwrap_or_default(),
                unknown7: 0,
                skin_tone: self
                    .customizations
                    .get(&CustomizationSlot::SkinTone)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param1.clone())
                    .unwrap_or_default(),
                face_paint: self
                    .customizations
                    .get(&CustomizationSlot::FacePattern)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param1.clone())
                    .unwrap_or_default(),
                facial_hair: self
                    .customizations
                    .get(&CustomizationSlot::FacialHair)
                    .and_then(|customization_guid| customizations.get(customization_guid))
                    .map(|customization| customization.customization_param1.clone())
                    .unwrap_or_default(),
                speed: character.stats.speed.total(),
                underage: false,
                member: self.member,
                moderator: false,
                temporary_appearance: 0,
                squads: Vec::new(),
                battle_class: self.active_battle_class,
                title: 0,
                unknown16: 0,
                unknown17: 0,
                effects: Vec::new(),
                mount_guid: player_mount_guid,
                unknown19: 0,
                unknown20: 0,
                wield_type: character.wield_type(),
                unknown22: 0.0,
                unknown23: 0,
                nameplate_image_id: NameplateImage::from_battle_class_guid(
                    self.active_battle_class,
                ),
            },
        })?];

        packets.append(&mut mount_packets);

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
    pub key: Option<String>,
    pub discriminant: u8,
    pub index: u16,
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
    pub animation_id: i32,
    pub character_type: CharacterType,
    pub mount_id: Option<u32>,
    pub cursor: Option<u8>,
    pub interact_radius: f32,
    pub auto_interact_radius: f32,
    pub wield_type: WieldType,
    pub tickable_procedures: HashMap<String, TickableProcedureConfig>,
    pub first_possible_procedures: Vec<String>,
    pub synchronize_with: Option<String>,
}

impl NpcTemplate {
    pub fn guid(&self, instance_guid: u64) -> u64 {
        npc_guid(self.discriminant, instance_guid, self.index)
    }

    pub fn to_character(
        &self,
        instance_guid: u64,
        keys_to_guid: &HashMap<&String, u64>,
    ) -> Character {
        Character {
            stats: CharacterStats {
                guid: self.guid(instance_guid),
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
                animation_id: self.animation_id,
                speed: CharacterStat {
                    base: 0.0,
                    mount_multiplier: 1.0,
                },
                jump_height_multiplier: CharacterStat {
                    base: 1.0,
                    mount_multiplier: 1.0,
                },
                cursor: self.cursor,
                name: None,
                squad_guid: None,
            },
            tickable_procedure_tracker: TickableProcedureTracker::new(
                self.tickable_procedures.clone(),
                self.first_possible_procedures.clone(),
            ),
            synchronize_with: self.synchronize_with.as_ref().map(|key| {
                keys_to_guid
                    .get(key)
                    .copied()
                    .unwrap_or_else(|| panic!("Tried to synchronize with unknown NPC {}", key))
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchmakingGroupStatus {
    OpenToAll,
    OpenToFriends,
}

pub type Chunk = (i32, i32);
pub type CharacterLocationIndex = (CharacterCategory, u64, Chunk);
pub type CharacterNameIndex = String;
pub type CharacterSquadIndex = u64;
pub type CharacterMatchmakingGroupIndex = (MatchmakingGroupStatus, i32, i32, Instant, u32);

#[derive(Clone)]
pub struct CharacterStat {
    pub base: f32,
    pub mount_multiplier: f32,
}

impl CharacterStat {
    pub fn total(&self) -> f32 {
        self.base * self.mount_multiplier
    }
}

#[derive(Clone)]
pub struct CharacterStats {
    guid: u64,
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
    pub character_type: CharacterType,
    pub mount_id: Option<u32>,
    pub interact_radius: f32,
    pub auto_interact_radius: f32,
    pub instance_guid: u64,
    pub animation_id: i32,
    pub speed: CharacterStat,
    pub jump_height_multiplier: CharacterStat,
    pub cursor: Option<u8>,
    pub name: Option<String>,
    pub squad_guid: Option<u64>,
    wield_type: (WieldType, WieldType),
    holstered: bool,
}

impl Guid<u64> for CharacterStats {
    fn guid(&self) -> u64 {
        self.guid
    }
}

#[derive(Clone)]
pub struct Character {
    pub stats: CharacterStats,
    tickable_procedure_tracker: TickableProcedureTracker,
    pub synchronize_with: Option<u64>,
}

impl
    IndexedGuid<
        u64,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
    > for Character
{
    fn guid(&self) -> u64 {
        self.stats.guid
    }

    fn index1(&self) -> CharacterLocationIndex {
        (
            match &self.stats.character_type {
                CharacterType::Player(player) => match player.ready {
                    true => CharacterCategory::PlayerReady,
                    false => CharacterCategory::PlayerUnready,
                },
                _ => match self.stats.auto_interact_radius > 0.0 {
                    true => CharacterCategory::NpcAutoInteractEnabled,
                    false => match self.tickable() {
                        true => CharacterCategory::NpcTickable,
                        false => CharacterCategory::NpcBasic,
                    },
                },
            },
            self.stats.instance_guid,
            Character::chunk(self.stats.pos.x, self.stats.pos.z),
        )
    }

    fn index2(&self) -> Option<CharacterNameIndex> {
        self.stats.name.clone()
    }

    fn index3(&self) -> Option<CharacterSquadIndex> {
        self.stats.squad_guid
    }

    fn index4(&self) -> Option<CharacterMatchmakingGroupIndex> {
        match &self.stats.character_type {
            CharacterType::Player(player) => player.matchmaking_group,
            _ => None,
        }
    }
}

impl Character {
    pub const MIN_CHUNK: (i32, i32) = (i32::MIN, i32::MIN);
    pub const MAX_CHUNK: (i32, i32) = (i32::MAX, i32::MAX);
    pub const DEFAULT_CHAT_TEXT_COLOR: Rgba = Rgba::new(255, 255, 255, 255);
    pub const DEFAULT_CHAT_BUBBLE_COLOR: Rgba = Rgba::new(240, 226, 212, 255);
    const CHUNK_SIZE: f32 = 200.0;

    pub fn new(
        guid: u64,
        pos: Pos,
        rot: Pos,
        scale: f32,
        character_type: CharacterType,
        mount_id: Option<u32>,
        cursor: Option<u8>,
        interact_radius: f32,
        auto_interact_radius: f32,
        instance_guid: u64,
        wield_type: WieldType,
        animation_id: i32,
        tickable_procedures: HashMap<String, TickableProcedureConfig>,
        first_possible_procedures: Vec<String>,
        synchronize_with: Option<u64>,
    ) -> Character {
        Character {
            stats: CharacterStats {
                guid,
                pos,
                rot,
                scale,
                character_type,
                mount_id,
                cursor,
                name: None,
                squad_guid: None,
                interact_radius,
                auto_interact_radius,
                instance_guid,
                wield_type: (wield_type, wield_type.holster()),
                holstered: false,
                animation_id,
                speed: CharacterStat {
                    base: 0.0,
                    mount_multiplier: 1.0,
                },
                jump_height_multiplier: CharacterStat {
                    base: 1.0,
                    mount_multiplier: 1.0,
                },
            },
            tickable_procedure_tracker: TickableProcedureTracker::new(
                tickable_procedures,
                first_possible_procedures,
            ),
            synchronize_with,
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
            stats: CharacterStats {
                guid: player_guid(guid),
                pos,
                rot,
                scale: 1.0,
                name: Some(format!("{}", data.name)),
                squad_guid: data.squad_guid,
                character_type: CharacterType::Player(Box::new(data)),
                mount_id: None,
                cursor: None,
                interact_radius: 0.0,
                auto_interact_radius: 0.0,
                instance_guid,
                wield_type: (wield_type, wield_type.holster()),
                holstered: false,
                animation_id: 0,
                speed: CharacterStat {
                    base: 0.0,
                    mount_multiplier: 1.0,
                },
                jump_height_multiplier: CharacterStat {
                    base: 1.0,
                    mount_multiplier: 1.0,
                },
            },
            tickable_procedure_tracker: TickableProcedureTracker::new(HashMap::new(), Vec::new()),
            synchronize_with: None,
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
            inner: RemoveStandard { guid: self.guid() },
        })?];

        if let Some(mount_id) = self.stats.mount_id {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: RemoveStandard {
                    guid: mount_guid(shorten_player_guid(self.guid())?, mount_id),
                },
            })?);
        }

        Ok(packets)
    }

    pub fn add_packets(
        &self,
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let packets = match &self.stats.character_type {
            CharacterType::AmbientNpc(ambient_npc) => ambient_npc.add_packets(self)?,
            CharacterType::Door(door) => door.add_packets(self)?,
            CharacterType::Transport(transport) => transport.add_packets(self)?,
            CharacterType::Player(player) => {
                player.add_packets(self, mount_configs, item_definitions, customizations)?
            }
            CharacterType::Fixture(house_guid, fixture) => fixture_packets(
                *house_guid,
                self.guid(),
                fixture,
                self.stats.pos,
                self.stats.rot,
                self.stats.scale,
            )?,
        };

        Ok(packets)
    }

    pub fn set_tickable_procedure_if_exists(
        &mut self,
        new_tickable_procedure: String,
        now: Instant,
    ) {
        self.tickable_procedure_tracker
            .set_procedure_if_exists(new_tickable_procedure, now);
    }

    pub fn tick(
        &mut self,
        now: Instant,
        nearby_player_guids: &[u32],
        nearby_players: &BTreeMap<u64, CharacterReadGuard>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        self.tickable_procedure_tracker.tick(
            &mut self.stats,
            now,
            nearby_player_guids,
            nearby_players,
        )
    }

    pub fn current_tickable_procedure(&self) -> Option<&String> {
        self.tickable_procedure_tracker.current_tickable_procedure()
    }

    pub fn last_procedure_change(&self) -> Instant {
        self.tickable_procedure_tracker.last_procedure_change()
    }

    pub fn wield_type(&self) -> WieldType {
        self.stats.wield_type.0
    }

    pub fn brandished_wield_type(&self) -> WieldType {
        if self.stats.holstered {
            self.stats.wield_type.1
        } else {
            self.stats.wield_type.0
        }
    }

    pub fn set_brandished_wield_type(&mut self, wield_type: WieldType) {
        self.stats.wield_type = (wield_type, wield_type.holster());
        self.stats.holstered = false;
    }

    pub fn brandish_or_holster(&mut self) {
        let (old_wield_type, new_wield_type) = self.stats.wield_type;
        self.stats.wield_type = (new_wield_type, old_wield_type);
        self.stats.holstered = !self.stats.holstered;
    }

    pub fn interact(
        &mut self,
        requester: u32,
        source_zone_guid: u64,
    ) -> WriteLockingBroadcastSupplier {
        let mut new_procedure = None;

        let broadcast_supplier = match &self.stats.character_type {
            CharacterType::AmbientNpc(ambient_npc) => {
                new_procedure = ambient_npc.interact(self);
                coerce_to_broadcast_supplier(|_| Ok(Vec::new()))
            }
            CharacterType::Door(door) => door.interact(requester, source_zone_guid),
            CharacterType::Transport(transport) => transport.interact(requester),
            _ => coerce_to_broadcast_supplier(|_| Ok(Vec::new())),
        };

        if let Some(procedure) = new_procedure {
            self.set_tickable_procedure_if_exists(procedure, Instant::now());
        }

        broadcast_supplier
    }

    fn tickable(&self) -> bool {
        self.tickable_procedure_tracker.tickable()
    }
}

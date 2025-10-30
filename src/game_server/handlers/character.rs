use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::{Duration, Instant},
};

use chrono::{DateTime, FixedOffset, Utc};
use enum_iterator::Sequence;
use rand::{seq::SliceRandom, thread_rng};
use rand_distr::{Distribution, WeightedAliasIndex};
use serde::Deserialize;

use crate::{
    game_server::{
        handlers::{
            dialog::DialogConfig,
            inventory::{attachments_from_equipped_items, wield_type_from_inventory},
            unique_guid::AMBIENT_NPC_DISCRIMINANT,
        },
        packets::{
            chat::{ActionBarTextColor, SendStringId},
            client_update::UpdateCredits,
            command::{DialogChoice, EnterDialog, PlaySoundIdOnTarget},
            item::{Attachment, BaseAttachmentGroup, EquipmentSlot, ItemDefinition, WieldType},
            minigame::ScoreEntry,
            player_update::{
                AddNotifications, AddNpc, AddPc, Customization, CustomizationSlot, Hostility, Icon,
                MoveOnRail, NameplateImage, NotificationData, NpcRelevance, PhysicsState,
                PlayCompositeEffect, QueueAnimation, RemoveGracefully, RemoveStandard,
                RemoveTemporaryModel, SetAnimation, SingleNotification, SingleNpcRelevance,
                UpdateSpeed, UpdateTemporaryModel,
            },
            tunnel::TunneledPacket,
            ui::{ExecuteScriptWithIntParams, ExecuteScriptWithStringParams},
            update_position::UpdatePlayerPosition,
            GamePacket, GuidTarget, Name, Pos, Rgba, Target,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
        TickableNpcSynchronization,
    },
    info,
};

use super::{
    distance3_pos,
    guid::{Guid, IndexedGuid},
    housing::fixture_packets,
    lock_enforcer::CharacterReadGuard,
    minigame::{MinigameTypeData, PlayerMinigameStats},
    mount::{spawn_mount_npc, MountConfig},
    unique_guid::{mount_guid, npc_guid, player_guid},
    zone::{teleport_anywhere, DestinationZoneInstance},
    WriteLockingBroadcastSupplier,
};

pub fn coerce_to_broadcast_supplier(
    f: impl FnOnce(&GameServer) -> Result<Vec<Broadcast>, ProcessPacketError> + 'static,
) -> WriteLockingBroadcastSupplier {
    Ok(Box::new(f))
}

pub const CHAT_BUBBLE_VISIBLE_RADIUS: f32 = 32.0;

const fn default_movement_animation_id() -> i32 {
    -1
}

const fn default_fade_millis() -> u32 {
    1000
}

const fn default_interact_radius() -> f32 {
    2.3
}

const fn default_move_to_interact_offset() -> f32 {
    2.2
}

const fn default_removal_delay_millis() -> u32 {
    5000
}

const fn default_scale() -> f32 {
    1.0
}

const fn default_true() -> bool {
    true
}

const fn default_weight() -> u32 {
    1
}

#[derive(Clone, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum CursorUpdate {
    #[default]
    Keep,
    Disable,
    Enable {
        new_cursor: u8,
    },
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RemovalMode {
    #[default]
    Immediate,
    Graceful {
        #[serde(default = "default_true")]
        enable_death_animation: bool,
        #[serde(default = "default_removal_delay_millis")]
        removal_delay_millis: u32,
        #[serde(default)]
        removal_effect_delay_millis: u32,
        #[serde(default)]
        removal_composite_effect_id: u32,
        #[serde(default = "default_fade_millis")]
        fade_duration_millis: u32,
    },
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum SpawnedState {
    #[default]
    Keep,
    Always,
    OnFirstStepTick,
    Despawn,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseNpcConfig {
    pub key: Option<String>,
    #[serde(default)]
    pub model_id: u32,
    #[serde(default)]
    pub possible_model_ids: Vec<u32>,
    #[serde(default)]
    pub texture_alias: String,
    #[serde(default)]
    pub name_id: u32,
    #[serde(default)]
    pub terrain_object_id: u32,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub pos: Pos,
    #[serde(default)]
    pub rot: Pos,
    #[serde(default)]
    pub possible_pos: Vec<Pos>,
    #[serde(default = "default_movement_animation_id")]
    pub stand_animation_id: i32,
    #[serde(default)]
    pub name_offset_x: f32,
    #[serde(default)]
    pub name_offset_y: f32,
    #[serde(default)]
    pub name_offset_z: f32,
    pub cursor: Option<u8>,
    #[serde(default = "default_true")]
    pub enable_interact_popup: bool,
    #[serde(default = "default_interact_radius")]
    pub interact_radius: f32,
    pub auto_interact_radius: Option<f32>,
    pub interact_popup_radius: Option<f32>,
    #[serde(default = "default_move_to_interact_offset")]
    pub move_to_interact_offset: f32,
    #[serde(default = "default_true")]
    pub show_name: bool,
    #[serde(default)]
    pub bounce_area_id: i32,
    #[serde(default)]
    pub physics: PhysicsState,
    #[serde(default = "default_true")]
    pub enable_gravity: bool,
    #[serde(default)]
    pub enable_tilt: bool,
    #[serde(default = "default_true")]
    pub use_terrain_model: bool,
    #[serde(default)]
    pub tickable_procedures: HashMap<String, TickableProcedureConfig>,
    #[serde(default)]
    pub first_possible_procedures: Vec<String>,
    pub synchronize_with: Option<String>,
    #[serde(default = "default_true")]
    pub is_spawned: bool,
}

#[derive(Clone)]
pub struct BaseNpc {
    pub texture_alias: String,
    pub name_id: u32,
    pub terrain_object_id: u32,
    pub name_offset_x: f32,
    pub name_offset_y: f32,
    pub name_offset_z: f32,
    pub enable_interact_popup: bool,
    pub interact_popup_radius: Option<f32>,
    pub show_name: bool,
    pub bounce_area_id: i32,
    pub enable_gravity: bool,
    pub enable_tilt: bool,
    pub use_terrain_model: bool,
    pub attachments: Vec<Attachment>,
}

impl BaseNpc {
    pub fn add_packets(
        &self,
        character: &CharacterStats,
        override_is_spawned: bool,
    ) -> Option<(AddNpc, SingleNpcRelevance)> {
        if !character.is_spawned && !override_is_spawned {
            return None;
        }
        Some((
            AddNpc {
                guid: Guid::guid(character),
                name_id: self.name_id,
                model_id: character.model_id,
                unknown3: false,
                chat_text_color: Character::DEFAULT_CHAT_TEXT_COLOR,
                chat_bubble_color: Character::DEFAULT_CHAT_BUBBLE_COLOR,
                chat_scale: 1,
                scale: character.scale,
                pos: character.pos,
                rot: character.rot,
                spawn_animation_id: 1,
                attachments: self.attachments.clone(),
                hostility: Hostility::Neutral,
                unknown10: 1,
                texture_alias: self.texture_alias.clone(),
                tint_name: "".to_string(),
                tint_id: 0,
                unknown11: true,
                offset_y: 0.0,
                composite_effect: 0,
                wield_type: character.wield_type(),
                name_override: "".to_string(),
                hide_name: !self.show_name,
                name_offset_x: self.name_offset_x,
                name_offset_y: self.name_offset_y,
                name_offset_z: self.name_offset_z,
                terrain_object_id: self.terrain_object_id,
                enable_attachments: !self.attachments.is_empty(),
                speed: character.speed.total(),
                unknown21: false,
                interactable_size_pct: 100,
                walk_animation_id: -1,
                sprint_animation_id: -1,
                stand_animation_id: character.stand_animation_id,
                unknown26: false,
                disable_gravity: !self.enable_gravity,
                sub_title_id: 0,
                one_shot_animation_id: 0,
                temporary_model: 0,
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
                physics: character.physics,
                interact_popup_radius: self
                    .interact_popup_radius
                    .unwrap_or(character.interact_radius),
                target: Target::default(),
                variables: vec![],
                rail_id: 0,
                rail_elapsed_seconds: 0.0,
                rail_offset: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                unknown54: 0,
                rail_unknown1: 0.0,
                rail_unknown2: 0.0,
                auto_interact_radius: character.auto_interact_radius,
                head_customization_override: "".to_string(),
                hair_customization_override: "".to_string(),
                body_customization_override: "".to_string(),
                override_terrain_model: !self.use_terrain_model,
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
                guid: Guid::guid(character),
                cursor: character.cursor,
                unknown1: false,
            },
        ))
    }
}

impl From<BaseNpcConfig> for BaseNpc {
    fn from(value: BaseNpcConfig) -> Self {
        BaseNpc {
            texture_alias: value.texture_alias,
            name_id: value.name_id,
            terrain_object_id: value.terrain_object_id,
            name_offset_x: value.name_offset_x,
            name_offset_y: value.name_offset_y,
            name_offset_z: value.name_offset_z,
            enable_interact_popup: value.enable_interact_popup,
            interact_popup_radius: value.interact_popup_radius,
            show_name: value.show_name,
            bounce_area_id: value.bounce_area_id,
            enable_gravity: value.enable_gravity,
            enable_tilt: value.enable_tilt,
            use_terrain_model: value.use_terrain_model,
            attachments: Vec::new(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerOneShotAction {
    pub player_one_shot_animation_id: Option<i32>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OneShotAction {
    #[serde(default, flatten)]
    pub player_action: PlayerOneShotAction,
    pub one_shot_action_composite_effect_id: Option<u32>,
    pub one_shot_action_animation_id: Option<i32>,
    #[serde(default)]
    pub award_credits: u32,
    #[serde(default)]
    pub removal_mode: RemovalMode,
    #[serde(default)]
    pub despawn_npc: bool,
    #[serde(default)]
    pub minigame_stage_group_guid: i32,
    pub dialog_config: Option<DialogConfig>,
    pub duration_millis: u64,
}

impl OneShotAction {
    pub fn apply(
        &self,
        character: &mut CharacterStats,
        nearby_player_guids: &[u32],
        requester: u32,
        player_stats: &mut Player,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let mut packets_for_all = Vec::new();
        let mut packets_for_sender = Vec::new();

        if self.despawn_npc {
            character.is_spawned = false;
            packets_for_all.extend(character.remove_packets(self.removal_mode));
        }

        if let Some(animation_id) = self.one_shot_action_animation_id {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: QueueAnimation {
                    character_guid: Guid::guid(character),
                    animation_id,
                    queue_pos: 0,
                    delay_seconds: 0.0,
                    duration_seconds: self.duration_millis as f32 / 1000.0,
                },
            }));
        }

        if let Some(composite_effect_id) = self.one_shot_action_composite_effect_id {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: PlayCompositeEffect {
                    guid: Guid::guid(character),
                    triggered_by_guid: 0,
                    composite_effect: composite_effect_id,
                    delay_millis: 0,
                    duration_millis: self.duration_millis as u32,
                    pos: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                },
            }));
        }

        if let Some(animation_id) = self.player_action.player_one_shot_animation_id {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: QueueAnimation {
                    character_guid: player_guid(requester),
                    animation_id,
                    queue_pos: 0,
                    delay_seconds: 0.0,
                    duration_seconds: self.duration_millis as f32 / 1000.0,
                },
            }));
        }

        if self.award_credits > 0 {
            let new_credits = player_stats.credits.saturating_add(self.award_credits);
            player_stats.credits = new_credits;
            packets_for_sender.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: UpdateCredits { new_credits },
            }));
        }

        if self.minigame_stage_group_guid > 0 {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: ExecuteScriptWithIntParams {
                    script_name: "MiniGameFlow.CreateMiniGameGroup".to_string(),
                    params: vec![self.minigame_stage_group_guid],
                },
            }));
        }

        if let Some(dialog) = &self.dialog_config {
            packets_for_sender.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: EnterDialog {
                    dialog_message_id: dialog.dialog_message_id,
                    speaker_animation_id: dialog.speaker_animation_id,
                    speaker_guid: Guid::guid(character),
                    enable_escape: true,
                    unknown4: 10.0,
                    dialog_choices: dialog
                        .choices
                        .as_ref()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|choice| DialogChoice {
                            button_id: choice.button_id,
                            unknown2: 0,
                            button_text_id: choice.button_text_id,
                            unknown4: 0,
                            unknown5: 0,
                        })
                        .collect(),
                    camera_placement: dialog.camera_placement,
                    look_at: dialog.look_at,
                    change_player_pos: false,
                    new_player_pos: Pos::default(),
                    unknown8: 10.0,
                    hide_players: !dialog.show_players,
                    unknown10: true,
                    unknown11: true,
                    zoom: dialog.zoom,
                    speaker_sound_id: dialog.speaker_sound_id,
                },
            }));
        }

        let broadcasts = vec![
            Broadcast::Multi(nearby_player_guids.to_vec(), packets_for_all),
            Broadcast::Single(requester, packets_for_sender),
        ];

        Ok(broadcasts)
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickableStep {
    pub speed: Option<f32>,
    pub new_pos_x: Option<f32>,
    pub new_pos_y: Option<f32>,
    pub new_pos_z: Option<f32>,
    pub new_rot_x: Option<f32>,
    pub new_rot_y: Option<f32>,
    pub new_rot_z: Option<f32>,
    #[serde(default)]
    pub new_pos_offset_x: f32,
    #[serde(default)]
    pub new_pos_offset_y: f32,
    #[serde(default)]
    pub new_pos_offset_z: f32,
    #[serde(default)]
    pub new_rot_offset_x: f32,
    #[serde(default)]
    pub new_rot_offset_y: f32,
    #[serde(default)]
    pub new_rot_offset_z: f32,
    pub animation_id: Option<i32>,
    pub one_shot_animation_id: Option<i32>,
    pub chat_message_id: Option<u32>,
    pub model_id: Option<u32>,
    pub sound_id: Option<u32>,
    pub rail_id: Option<u32>,
    pub composite_effect_id: Option<u32>,
    #[serde(default)]
    pub effect_delay_millis: u32,
    #[serde(default)]
    pub removal_mode: RemovalMode,
    #[serde(default)]
    pub spawned_state: SpawnedState,
    #[serde(default)]
    pub cursor: CursorUpdate,
    pub duration_millis: u64,
}

impl TickableStep {
    pub fn reselect_possible_pos(
        &self,
        character: &CharacterStats,
    ) -> Option<UpdatePlayerPosition> {
        if character.possible_pos.is_empty() {
            return None;
        }

        character
            .possible_pos
            .choose(&mut thread_rng())
            .map(|pos| UpdatePlayerPosition {
                guid: Guid::guid(character),
                pos_x: pos.x,
                pos_y: pos.y,
                pos_z: pos.z,
                rot_x: character.rot.x,
                rot_y: character.rot.y,
                rot_z: character.rot.z,
                character_state: 1,
                unknown: 0,
            })
    }

    pub fn new_pos(&self, current_pos: Pos) -> Pos {
        Pos {
            x: self.new_pos_x.unwrap_or(current_pos.x) + self.new_pos_offset_x,
            y: self.new_pos_y.unwrap_or(current_pos.y) + self.new_pos_offset_y,
            z: self.new_pos_z.unwrap_or(current_pos.z) + self.new_pos_offset_z,
            w: current_pos.w,
        }
    }

    pub fn new_rot(&self, current_rot: Pos) -> Pos {
        Pos {
            x: self.new_rot_x.unwrap_or(current_rot.x) + self.new_rot_offset_x,
            y: self.new_rot_y.unwrap_or(current_rot.y) + self.new_rot_offset_y,
            z: self.new_rot_z.unwrap_or(current_rot.z) + self.new_rot_offset_z,
            w: current_rot.w,
        }
    }

    pub fn apply(
        &self,
        character: &mut CharacterStats,
        nearby_player_guids: &[u32],
        nearby_players: &BTreeMap<u64, CharacterReadGuard>,
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> (Vec<Broadcast>, Option<UpdatePlayerPosition>) {
        let mut packets_for_all = Vec::new();
        let mut update_pos: Option<UpdatePlayerPosition> = None;

        match self.spawned_state {
            SpawnedState::Always => {
                if !character.is_spawned {
                    character.is_spawned = true;
                    update_pos = self.reselect_possible_pos(character);
                    packets_for_all.extend(character.add_packets(
                        false,
                        mount_configs,
                        item_definitions,
                        customizations,
                    ));
                }
            }
            SpawnedState::OnFirstStepTick => {
                if !character.is_spawned {
                    // Spawn the character without updating its state to prevent it from being visible
                    // to players joining the room mid-step
                    update_pos = self.reselect_possible_pos(character);
                    packets_for_all.extend(character.add_packets(
                        true, // Override is_spawned
                        mount_configs,
                        item_definitions,
                        customizations,
                    ));
                }
            }
            SpawnedState::Despawn => {
                // Skip checking if the character is spawned before despawning it and instead check if
                // its state needs updating as OnFirstStepTick doesn't maintain states
                character.is_spawned = false;
                packets_for_all.extend(character.remove_packets(self.removal_mode));
            }
            SpawnedState::Keep => {}
        }

        if let Some(model_id) = self.model_id {
            if let Some(temporary_model_id) = character.temporary_model_id {
                packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: RemoveTemporaryModel {
                        guid: Guid::guid(character),
                        model_id: temporary_model_id,
                    },
                }));
            }

            character.temporary_model_id = Some(model_id);
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: UpdateTemporaryModel {
                    model_id,
                    guid: Guid::guid(character),
                },
            }));
        }

        if let Some(composite_effect_id) = self.composite_effect_id {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: PlayCompositeEffect {
                    guid: Guid::guid(character),
                    triggered_by_guid: 0,
                    composite_effect: composite_effect_id,
                    delay_millis: self.effect_delay_millis,
                    duration_millis: self.duration_millis as u32,
                    pos: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                },
            }));
        }

        if let Some(rail_id) = self.rail_id {
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: MoveOnRail {
                    guid: Guid::guid(character),
                    rail_id,
                    elapsed_seconds: 0.0,
                    rail_offset: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                },
            }));
        }

        if let Some(speed) = self.speed {
            character.speed.base = speed;
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: UpdateSpeed {
                    guid: Guid::guid(character),
                    speed,
                },
            }));
        }

        let new_pos = self.new_pos(character.pos);
        let new_rot = self.new_rot(character.rot);
        if new_pos != character.pos || new_rot != character.rot {
            update_pos = Some(UpdatePlayerPosition {
                guid: Guid::guid(character),
                pos_x: new_pos.x,
                pos_y: new_pos.y,
                pos_z: new_pos.z,
                rot_x: new_rot.x,
                rot_y: new_rot.y,
                rot_z: new_rot.z,
                character_state: 1,
                unknown: 0,
            });
        }

        if let Some(animation_id) = self.animation_id {
            character.stand_animation_id = animation_id;
            packets_for_all.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SetAnimation {
                    character_guid: Guid::guid(character),
                    animation_id,
                    animation_group_id: -1,
                    override_animation: true,
                },
            }));
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
            }));
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
            }));
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
            }));
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
            })];

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

        (broadcasts, update_pos)
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TickableProcedureReference {
    #[serde(default)]
    pub procedure: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickableProcedureConfig {
    #[serde(flatten)]
    pub reference: TickableProcedureReference,
    pub steps: Vec<TickableStep>,
    #[serde(default)]
    pub next_possible_procedures: Vec<TickableProcedureReference>,
    #[serde(default = "default_true")]
    pub is_interruptible: bool,
}

pub enum TickResult {
    TickedCurrentProcedure(Vec<Broadcast>, Option<UpdatePlayerPosition>),
    MustChangeProcedure(String),
}

#[derive(Clone)]
pub struct TickableProcedure {
    steps: Vec<TickableStep>,
    current_step: Option<(usize, Instant)>,
    distribution: WeightedAliasIndex<u32>,
    next_possible_procedures: Vec<String>,
    is_interruptible: bool,
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
                        .map(|procedure| procedure.reference.weight)
                        .collect(),
                ),
                all_procedures.keys().cloned().collect(),
            )
        } else {
            let weights = config
                .next_possible_procedures
                .iter()
                .map(|proc_ref| {
                    if !all_procedures.contains_key(&proc_ref.procedure) {
                        panic!("Reference to unknown procedure: {}", proc_ref.procedure);
                    }
                    proc_ref.weight
                })
                .collect();
            let references = config
                .next_possible_procedures
                .iter()
                .map(|proc_ref| proc_ref.procedure.clone())
                .collect();
            (WeightedAliasIndex::new(weights), references)
        };

        let procedure = TickableProcedure {
            steps: config.steps,
            current_step: None,
            distribution: distribution.expect("Couldn't create weighted alias index"),
            next_possible_procedures,
            is_interruptible: config.is_interruptible,
        };

        procedure.panic_if_removal_exceeds_duration();

        procedure
    }

    pub fn tick(
        &mut self,
        character: &mut CharacterStats,
        now: Instant,
        nearby_player_guids: &[u32],
        nearby_players: &BTreeMap<u64, CharacterReadGuard>,
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
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

                let (broadcasts, update_pos) = self.steps[new_step_index].apply(
                    character,
                    nearby_player_guids,
                    nearby_players,
                    mount_configs,
                    item_definitions,
                    customizations,
                );
                TickResult::TickedCurrentProcedure(broadcasts, update_pos)
            }
        } else {
            TickResult::TickedCurrentProcedure(Vec::new(), None)
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

    pub fn is_interruptible(&self) -> bool {
        self.is_interruptible
    }

    fn panic_if_removal_exceeds_duration(&self) {
        for step in &self.steps {
            if let RemovalMode::Graceful {
                removal_delay_millis,
                removal_effect_delay_millis,
                fade_duration_millis,
                ..
            } = step.removal_mode
            {
                let total_removal_time =
                    removal_delay_millis + removal_effect_delay_millis + fade_duration_millis;

                if total_removal_time > step.duration_millis as u32 {
                    panic!(
                        "(Removal delay: {}) + (Fade duration: {}) exceeded (Step duration: {})",
                        removal_delay_millis, fade_duration_millis, step.duration_millis
                    );
                }
            }
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
                        .map(|procedure| procedure.reference.weight)
                        .collect();
                    (weights, procedures.keys().collect())
                } else {
                    let weights = first_possible_procedures
                        .iter()
                        .map(|procedure_key| {
                            if let Some(procedure) = procedures.get(procedure_key) {
                                procedure.reference.weight
                            } else {
                                panic!("Reference to unknown procedure {procedure_key}");
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
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> (Vec<Broadcast>, Option<UpdatePlayerPosition>) {
        if self.procedures.is_empty() {
            return (Vec::new(), None);
        }

        let mut current_procedure = self
            .procedures
            .get_mut(&self.current_procedure_key)
            .expect("Missing procedure");
        loop {
            let tick_result = current_procedure.tick(
                character,
                now,
                nearby_player_guids,
                nearby_players,
                mount_configs,
                item_definitions,
                customizations,
            );
            if let TickResult::TickedCurrentProcedure(broadcasts, update_pos) = tick_result {
                break (broadcasts, update_pos);
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

pub trait NpcConfig: Into<CharacterType> {
    const DISCRIMINANT: u8;
    const DEFAULT_AUTO_INTERACT_RADIUS: f32;

    fn base_config(&self) -> &BaseNpcConfig;
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmbientNpcConfig {
    #[serde(flatten)]
    pub base_npc: BaseNpcConfig,
    pub procedure_on_interact: Option<Vec<TickableProcedureReference>>,
    pub one_shot_action_on_interact: Option<OneShotAction>,
}

impl NpcConfig for AmbientNpcConfig {
    const DISCRIMINANT: u8 = AMBIENT_NPC_DISCRIMINANT;
    const DEFAULT_AUTO_INTERACT_RADIUS: f32 = 0.0;

    fn base_config(&self) -> &BaseNpcConfig {
        &self.base_npc
    }
}

impl From<AmbientNpcConfig> for CharacterType {
    fn from(value: AmbientNpcConfig) -> Self {
        CharacterType::AmbientNpc(value.into())
    }
}

#[derive(Clone)]
pub struct AmbientNpc {
    pub base_npc: BaseNpc,
    pub procedure_on_interact: Option<Vec<TickableProcedureReference>>,
    pub one_shot_action_on_interact: Option<OneShotAction>,
}

impl AmbientNpc {
    pub fn add_packets(
        &self,
        character: &CharacterStats,
        override_is_spawned: bool,
    ) -> Vec<Vec<u8>> {
        let Some((add_npc, enable_interaction)) =
            self.base_npc.add_packets(character, override_is_spawned)
        else {
            return Vec::new();
        };
        let packets = vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: add_npc,
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NpcRelevance {
                    new_states: vec![enable_interaction],
                },
            }),
        ];

        packets
    }

    pub fn interact(
        &self,
        character: &mut Character,
        nearby_player_guids: &[u32],
        requester: u32,
        player_stats: &mut Player,
    ) -> (Option<String>, WriteLockingBroadcastSupplier) {
        if let Some(active_procedure_key) = character.current_tickable_procedure() {
            if let Some(active_procedure) = character
                .tickable_procedure_tracker
                .procedures
                .get(active_procedure_key)
            {
                if !active_procedure.is_interruptible() {
                    let empty_supplier = coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                    return (None, empty_supplier);
                }
            }
        }

        let procedure = self.procedure_on_interact.as_ref().map(|options| {
            let weights: Vec<u32> = options.iter().map(|p| p.weight).collect();
            let distribution =
                WeightedAliasIndex::new(weights).expect("Failed to build alias index");
            let index = distribution.sample(&mut thread_rng());
            options[index].procedure.clone()
        });

        let packets = self
            .one_shot_action_on_interact
            .as_ref()
            .map_or(Ok(Vec::new()), |action| {
                action.apply(
                    &mut character.stats,
                    nearby_player_guids,
                    requester,
                    player_stats,
                )
            });

        let broadcast_supplier = coerce_to_broadcast_supplier(move |_| packets);
        (procedure, broadcast_supplier)
    }
}

impl From<AmbientNpcConfig> for AmbientNpc {
    fn from(value: AmbientNpcConfig) -> Self {
        AmbientNpc {
            base_npc: value.base_npc.into(),
            procedure_on_interact: value.procedure_on_interact,
            one_shot_action_on_interact: value.one_shot_action_on_interact,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoorConfig {
    #[serde(flatten)]
    pub base_npc: BaseNpcConfig,
    pub destination_pos: Pos,
    pub destination_rot: Pos,
    #[serde(default)]
    pub destination_zone: DestinationZoneInstance,
}

impl NpcConfig for DoorConfig {
    const DISCRIMINANT: u8 = AMBIENT_NPC_DISCRIMINANT;
    const DEFAULT_AUTO_INTERACT_RADIUS: f32 = 1.5;

    fn base_config(&self) -> &BaseNpcConfig {
        &self.base_npc
    }
}

impl From<DoorConfig> for CharacterType {
    fn from(value: DoorConfig) -> Self {
        CharacterType::Door(value.into())
    }
}

#[derive(Clone)]
pub struct Door {
    pub base_npc: BaseNpc,
    pub destination_pos: Pos,
    pub destination_rot: Pos,
    pub destination_zone: DestinationZoneInstance,
}

impl Door {
    pub fn add_packets(
        &self,
        character: &CharacterStats,
        override_is_spawned: bool,
    ) -> Vec<Vec<u8>> {
        let Some((mut add_npc, mut enable_interaction)) =
            self.base_npc.add_packets(character, override_is_spawned)
        else {
            return Vec::new();
        };
        add_npc.disable_interact_popup = true;
        enable_interaction.cursor = enable_interaction.cursor.or(Some(55));
        let packets = vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: add_npc,
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NpcRelevance {
                    new_states: vec![enable_interaction],
                },
            }),
        ];

        packets
    }

    pub fn interact(&self, requester: u32) -> WriteLockingBroadcastSupplier {
        teleport_anywhere(
            self.destination_pos,
            self.destination_rot,
            self.destination_zone,
            requester,
        )
    }
}

impl From<DoorConfig> for Door {
    fn from(value: DoorConfig) -> Self {
        Door {
            base_npc: value.base_npc.into(),
            destination_pos: value.destination_pos,
            destination_rot: value.destination_rot,
            destination_zone: value.destination_zone,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportConfig {
    #[serde(flatten)]
    pub base_npc: BaseNpcConfig,
    pub show_icon: bool,
    pub large_icon: bool,
    pub show_hover_description: bool,
}

impl NpcConfig for TransportConfig {
    const DISCRIMINANT: u8 = AMBIENT_NPC_DISCRIMINANT;
    const DEFAULT_AUTO_INTERACT_RADIUS: f32 = 0.0;

    fn base_config(&self) -> &BaseNpcConfig {
        &self.base_npc
    }
}

impl From<TransportConfig> for CharacterType {
    fn from(value: TransportConfig) -> Self {
        CharacterType::Transport(value.into())
    }
}

#[derive(Clone)]
pub struct Transport {
    pub base_npc: BaseNpc,
    pub show_icon: bool,
    pub large_icon: bool,
    pub show_hover_description: bool,
}

impl Transport {
    pub fn add_packets(
        &self,
        character: &CharacterStats,
        override_is_spawned: bool,
    ) -> Vec<Vec<u8>> {
        let Some((mut add_npc, enable_interaction)) =
            self.base_npc.add_packets(character, override_is_spawned)
        else {
            return Vec::new();
        };
        add_npc.hover_description = if self.show_hover_description {
            self.base_npc.name_id
        } else {
            0
        };
        let packets = vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: add_npc,
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NpcRelevance {
                    new_states: vec![enable_interaction],
                },
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AddNotifications {
                    notifications: vec![SingleNotification {
                        guid: Guid::guid(character),
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
            }),
        ];

        packets
    }

    pub fn interact(&self, requester: u32) -> WriteLockingBroadcastSupplier {
        coerce_to_broadcast_supplier(move |_| {
            Ok(vec![Broadcast::Single(
                requester,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: false,
                    inner: ExecuteScriptWithStringParams {
                        script_name: "UIGlobal.ShowGalaxyMap".to_string(),
                        params: vec![],
                    },
                })],
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

pub type EquippedItemMap = BTreeMap<EquipmentSlot, u32>;

#[derive(Clone)]
pub struct BattleClass {
    pub items: EquippedItemMap,
}

#[derive(Clone)]
pub struct PlayerInventory {
    battle_classes: BTreeMap<u32, BattleClass>,
    pub active_battle_class: u32,
    temporary_items: BTreeMap<EquipmentSlot, Option<u32>>,
    inventory: BTreeSet<u32>,
}

impl PlayerInventory {
    pub fn new(
        battle_classes: BTreeMap<u32, BattleClass>,
        active_battle_class: u32,
        inventory: BTreeSet<u32>,
    ) -> Self {
        PlayerInventory {
            battle_classes,
            active_battle_class,
            temporary_items: BTreeMap::new(),
            inventory,
        }
    }

    pub fn equipped_items(&self, battle_class: u32) -> EquippedItemMap {
        let mut items = self
            .battle_classes
            .get(&battle_class)
            .map(|battle_class| &battle_class.items)
            .cloned()
            .unwrap_or_default();
        self.temporary_items
            .iter()
            .for_each(|(slot, item_guid_if_any)| {
                match item_guid_if_any {
                    Some(item_guid) => items.insert(*slot, *item_guid),
                    None => items.remove(slot),
                };
            });
        items
    }

    pub fn equipped_item(&self, battle_class: u32, slot: EquipmentSlot) -> Option<u32> {
        self.temporary_items
            .get(&slot)
            .copied()
            .or_else(|| {
                Some(
                    self.battle_classes
                        .get(&battle_class)
                        .and_then(|battle_class| battle_class.items.get(&slot).copied()),
                )
            })
            .unwrap_or_default()
    }

    pub fn equip_item(
        &mut self,
        battle_class_guid: u32,
        slot: EquipmentSlot,
        item_guid: u32,
    ) -> Result<bool, ProcessPacketError> {
        if !self.inventory.contains(&item_guid) {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player doesn't own item {item_guid} and can't equip it in slot {slot:?} of battle class {battle_class_guid}",
                ),
            ));
        }

        let Some(battle_class) = self.battle_classes.get_mut(&battle_class_guid) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player doesn't own battle class {battle_class_guid} and can't equip item {item_guid} in slot {slot:?}"
                ),
            ));
        };

        battle_class.items.insert(slot, item_guid);
        Ok(battle_class_guid == self.active_battle_class
            && !self.temporary_items.contains_key(&slot))
    }

    pub fn unequip_item(
        &mut self,
        battle_class_guid: u32,
        slot: EquipmentSlot,
    ) -> Result<bool, ProcessPacketError> {
        let Some(battle_class) = self.battle_classes.get_mut(&battle_class_guid) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player doesn't own battle class {battle_class_guid} and can't unequip slot {slot:?}"
                ),
            ));
        };

        Ok(battle_class_guid == self.active_battle_class
            && battle_class.items.remove(&slot).is_some()
            && !self.temporary_items.contains_key(&slot))
    }

    pub fn equip_item_temporarily(&mut self, slot: EquipmentSlot, item_guid: Option<u32>) {
        self.temporary_items.insert(slot, item_guid);
    }

    pub fn clear_temporary_items(&mut self) {
        self.temporary_items.clear();
    }

    pub fn owns_item(&self, item_guid: u32) -> bool {
        self.inventory.contains(&item_guid)
    }
}

#[derive(Clone, Default)]
pub struct MinigameWinStatus(pub Option<DateTime<FixedOffset>>);

impl MinigameWinStatus {
    pub fn set_won(&mut self, won: bool) {
        if won {
            self.0 = Some(Utc::now().fixed_offset());
        }
    }

    pub fn set_win_time(&mut self, won_time: DateTime<FixedOffset>) {
        self.0 = Some(won_time);
    }

    pub fn won(&self) -> bool {
        self.0.is_some()
    }
}

#[derive(Clone)]
pub struct MinigameStatus {
    pub group: MinigameMatchmakingGroup,
    pub teleported_to_game: bool,
    pub game_created: bool,
    pub win_status: MinigameWinStatus,
    pub score_entries: Vec<ScoreEntry>,
    pub total_score: i32,
    pub awarded_credits: u32,
    pub type_data: MinigameTypeData,
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
    pub inventory: PlayerInventory,
    pub customizations: BTreeMap<CustomizationSlot, u32>,
    pub minigame_stats: PlayerMinigameStats,
    pub minigame_status: Option<MinigameStatus>,
    pub update_previous_location_on_leave: bool,
    pub previous_location: PreviousLocation,
}

impl Player {
    pub fn add_packets(
        &self,
        character: &CharacterStats,
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> Vec<Vec<u8>> {
        if !self.ready {
            return Vec::new();
        }

        let mut mount_packets = Vec::new();
        let mut player_mount_guid = 0;
        if let Some(CharacterMount {
            mount_id,
            mount_guid,
        }) = character.mount
        {
            player_mount_guid = mount_guid;
            if let Some(mount_config) = mount_configs.get(&mount_id) {
                mount_packets.append(&mut spawn_mount_npc(
                    player_mount_guid,
                    Guid::guid(character),
                    mount_config,
                    character.pos,
                    character.rot,
                    false,
                ));
            } else {
                info!(
                    "Character {} is mounted on unknown mount ID {}",
                    Guid::guid(character),
                    mount_id
                );
            }
        }

        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AddPc {
                guid: Guid::guid(character),
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
                pos: character.pos,
                rot: character.rot,
                attachments: attachments_from_equipped_items(
                    &self
                        .inventory
                        .equipped_items(self.inventory.active_battle_class),
                    item_definitions,
                )
                .into_iter()
                .map(|attachment| attachment.into())
                .collect(),
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
                speed: character.speed.total(),
                underage: false,
                member: self.member,
                moderator: false,
                temporary_model: 0,
                squads: Vec::new(),
                battle_class: self.inventory.active_battle_class,
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
                    self.inventory.active_battle_class,
                ),
            },
        })];

        packets.append(&mut mount_packets);

        packets
    }
}

pub struct PreviousFixture {
    pub pos: Pos,
    pub rot: Pos,
    pub scale: f32,
    pub item_def_id: u32,
    pub model_id: u32,
    pub texture_alias: String,
}

impl PreviousFixture {
    pub fn as_current_fixture(&self) -> CurrentFixture {
        CurrentFixture {
            item_def_id: self.item_def_id,
            model_id: self.model_id,
            texture_alias: self.texture_alias.clone(),
        }
    }
}

#[derive(Clone)]
pub struct CurrentFixture {
    pub item_def_id: u32,
    pub model_id: u32,
    pub texture_alias: String,
}

#[derive(Clone)]
pub enum CharacterType {
    AmbientNpc(AmbientNpc),
    Door(Door),
    Transport(Transport),
    Player(Box<Player>),
    Fixture(u64, CurrentFixture),
}

#[derive(Copy, Clone, Eq, PartialOrd, PartialEq, Ord, Sequence)]
pub enum CharacterCategory {
    PlayerReady,
    PlayerUnready,
    NpcTickable(TickableNpcSynchronization),
    NpcBasic,
}

#[derive(Clone)]
pub struct NpcTemplate {
    pub key: Option<String>,
    pub discriminant: u8,
    pub index: u16,
    pub model_id: u32,
    pub pos: Pos,
    pub rot: Pos,
    pub possible_pos: Vec<Pos>,
    pub scale: f32,
    pub stand_animation_id: i32,
    pub character_type: CharacterType,
    pub mount_id: Option<u32>,
    pub cursor: Option<u8>,
    pub interact_radius: f32,
    pub auto_interact_radius: f32,
    pub move_to_interact_offset: f32,
    pub wield_type: WieldType,
    pub tickable_procedures: HashMap<String, TickableProcedureConfig>,
    pub first_possible_procedures: Vec<String>,
    pub synchronize_with: Option<String>,
    pub is_spawned: bool,
    pub physics: PhysicsState,
}

impl NpcTemplate {
    pub fn from_config<T: NpcConfig>(config: T, index: u16) -> Self {
        let mut rng = thread_rng();
        NpcTemplate {
            key: config.base_config().key.clone(),
            discriminant: T::DISCRIMINANT,
            index,
            model_id: config
                .base_config()
                .possible_model_ids
                .choose(&mut rng)
                .copied()
                .unwrap_or(config.base_config().model_id),
            pos: config
                .base_config()
                .possible_pos
                .choose(&mut rng)
                .cloned()
                .unwrap_or(config.base_config().pos),
            rot: config.base_config().rot,
            possible_pos: config.base_config().possible_pos.clone(),
            scale: config.base_config().scale,
            tickable_procedures: config.base_config().tickable_procedures.clone(),
            first_possible_procedures: config.base_config().first_possible_procedures.clone(),
            synchronize_with: config.base_config().synchronize_with.clone(),
            stand_animation_id: config.base_config().stand_animation_id,
            cursor: config.base_config().cursor,
            interact_radius: config.base_config().interact_radius,
            auto_interact_radius: config
                .base_config()
                .auto_interact_radius
                .unwrap_or(T::DEFAULT_AUTO_INTERACT_RADIUS),
            move_to_interact_offset: config.base_config().move_to_interact_offset,
            is_spawned: config.base_config().is_spawned,
            physics: config.base_config().physics,
            character_type: config.into(),
            mount_id: None,
            wield_type: WieldType::None,
        }
    }

    pub fn guid(&self, instance_guid: u64) -> u64 {
        npc_guid(self.discriminant, instance_guid, self.index)
    }

    pub fn to_character(
        &self,
        instance_guid: u64,
        chunk_size: u16,
        keys_to_guid: &HashMap<&String, u64>,
    ) -> Character {
        let guid = self.guid(instance_guid);
        Character {
            stats: CharacterStats {
                guid,
                model_id: self.model_id,
                pos: self.pos,
                rot: self.rot,
                possible_pos: self.possible_pos.clone(),
                chunk_size,
                scale: self.scale,
                character_type: self.character_type.clone(),
                mount: self.mount_id.map(|mount_id| CharacterMount {
                    mount_id,
                    mount_guid: mount_guid(guid),
                }),
                interact_radius: self.interact_radius,
                auto_interact_radius: self.auto_interact_radius,
                move_to_interact_offset: self.move_to_interact_offset,
                instance_guid,
                wield_type: (self.wield_type, self.wield_type.holster()),
                holstered: false,
                stand_animation_id: self.stand_animation_id,
                temporary_model_id: None,
                speed: CharacterStat {
                    base: 0.0,
                    mount_multiplier: 1.0,
                },
                jump_height_multiplier: CharacterStat {
                    base: 1.0,
                    mount_multiplier: 1.0,
                },
                cursor: self.cursor,
                is_spawned: self.is_spawned,
                physics: self.physics,
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
                    .unwrap_or_else(|| panic!("Tried to synchronize with unknown NPC {key}"))
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    pub size: u16,
}
pub type CharacterLocationIndex = (CharacterCategory, u64, Chunk);
pub type CharacterNameIndex = String;
pub type CharacterSquadIndex = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MinigameMatchmakingGroup {
    pub stage_group_guid: i32,
    pub stage_guid: i32,
    pub creation_time: Instant,
    pub owner_guid: u32,
}

pub type CharacterMatchmakingGroupIndex = MinigameMatchmakingGroup;
pub type CharacterSynchronizationIndex = u64;

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
pub struct CharacterMount {
    pub mount_id: u32,
    pub mount_guid: u64,
}

#[derive(Clone)]
pub struct CharacterStats {
    guid: u64,
    pub model_id: u32,
    pub pos: Pos,
    pub rot: Pos,
    pub possible_pos: Vec<Pos>,
    pub chunk_size: u16,
    pub scale: f32,
    pub character_type: CharacterType,
    pub mount: Option<CharacterMount>,
    pub interact_radius: f32,
    pub auto_interact_radius: f32,
    pub move_to_interact_offset: f32,
    pub instance_guid: u64,
    pub temporary_model_id: Option<u32>,
    pub stand_animation_id: i32,
    pub speed: CharacterStat,
    pub jump_height_multiplier: CharacterStat,
    pub cursor: Option<u8>,
    pub is_spawned: bool,
    pub physics: PhysicsState,
    pub name: Option<String>,
    pub squad_guid: Option<u64>,
    wield_type: (WieldType, WieldType),
    holstered: bool,
}

impl CharacterStats {
    pub fn add_packets(
        &self,
        override_is_spawned: bool,
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> Vec<Vec<u8>> {
        match &self.character_type {
            CharacterType::AmbientNpc(ambient_npc) => {
                ambient_npc.add_packets(self, override_is_spawned)
            }
            CharacterType::Door(door) => door.add_packets(self, override_is_spawned),
            CharacterType::Transport(transport) => transport.add_packets(self, override_is_spawned),
            CharacterType::Player(player) => {
                player.add_packets(self, mount_configs, item_definitions, customizations)
            }
            CharacterType::Fixture(house_guid, fixture) => fixture_packets(
                *house_guid,
                Guid::guid(self),
                fixture,
                self.pos,
                self.rot,
                self.scale,
            ),
        }
    }

    pub fn remove_packets(&self, mode: RemovalMode) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        if let Some(temporary_model_id) = self.temporary_model_id {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: RemoveTemporaryModel {
                    guid: Guid::guid(self),
                    model_id: temporary_model_id,
                },
            }));
        }

        packets.push(match mode {
            RemovalMode::Immediate => GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: RemoveStandard {
                    guid: Guid::guid(self),
                },
            }),
            RemovalMode::Graceful {
                enable_death_animation,
                removal_delay_millis,
                removal_effect_delay_millis,
                removal_composite_effect_id,
                fade_duration_millis,
            } => GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: RemoveGracefully {
                    guid: Guid::guid(self),
                    use_death_animation: enable_death_animation,
                    delay_millis: removal_delay_millis,
                    composite_effect_delay_millis: removal_effect_delay_millis,
                    composite_effect: removal_composite_effect_id,
                    fade_duration_millis,
                },
            }),
        });

        if let Some(CharacterMount { mount_guid, .. }) = self.mount {
            packets.push(match mode {
                RemovalMode::Immediate => GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: RemoveStandard { guid: mount_guid },
                }),
                RemovalMode::Graceful {
                    enable_death_animation,
                    removal_delay_millis,
                    removal_effect_delay_millis,
                    removal_composite_effect_id,
                    fade_duration_millis,
                } => GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: RemoveGracefully {
                        guid: Guid::guid(self),
                        use_death_animation: enable_death_animation,
                        delay_millis: removal_delay_millis,
                        composite_effect_delay_millis: removal_effect_delay_millis,
                        composite_effect: removal_composite_effect_id,
                        fade_duration_millis,
                    },
                }),
            });
        }
        packets
    }

    pub fn wield_type(&self) -> WieldType {
        self.wield_type.0
    }
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
        CharacterSynchronizationIndex,
    > for Character
{
    fn guid(&self) -> u64 {
        self.stats.guid
    }

    fn index1(&self) -> CharacterLocationIndex {
        let tickable_synchronization = match self.synchronize_with {
            Some(_) => TickableNpcSynchronization::Synchronized,
            None => TickableNpcSynchronization::Unsynchronized,
        };
        (
            match &self.stats.character_type {
                CharacterType::Player(player) => match player.ready {
                    true => CharacterCategory::PlayerReady,
                    false => CharacterCategory::PlayerUnready,
                },
                _ => match self.tickable() {
                    true => CharacterCategory::NpcTickable(tickable_synchronization),
                    false => CharacterCategory::NpcBasic,
                },
            },
            self.stats.instance_guid,
            Character::chunk(self.stats.pos.x, self.stats.pos.z, self.stats.chunk_size),
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
            CharacterType::Player(player) => {
                player.minigame_status.as_ref().map(|status| status.group)
            }
            _ => None,
        }
    }

    fn index5(&self) -> Option<CharacterSynchronizationIndex> {
        self.synchronize_with
    }
}

impl Character {
    pub const MIN_CHUNK: Chunk = Chunk {
        x: i32::MIN,
        z: i32::MIN,
        size: u16::MIN,
    };
    pub const MAX_CHUNK: Chunk = Chunk {
        x: i32::MAX,
        z: i32::MAX,
        size: u16::MAX,
    };
    pub const DEFAULT_CHAT_TEXT_COLOR: Rgba = Rgba::new(255, 255, 255, 255);
    pub const DEFAULT_CHAT_BUBBLE_COLOR: Rgba = Rgba::new(240, 226, 212, 255);

    pub fn new(
        guid: u64,
        model_id: u32,
        pos: Pos,
        rot: Pos,
        chunk_size: u16,
        scale: f32,
        character_type: CharacterType,
        mount_id: Option<CharacterMount>,
        cursor: Option<u8>,
        interact_radius: f32,
        auto_interact_radius: f32,
        move_to_interact_offset: f32,
        instance_guid: u64,
        wield_type: WieldType,
        stand_animation_id: i32,
        tickable_procedures: HashMap<String, TickableProcedureConfig>,
        first_possible_procedures: Vec<String>,
        synchronize_with: Option<u64>,
    ) -> Character {
        Character {
            stats: CharacterStats {
                guid,
                model_id,
                pos,
                rot,
                possible_pos: vec![],
                chunk_size,
                scale,
                character_type,
                mount: mount_id,
                cursor,
                is_spawned: true,
                physics: PhysicsState::default(),
                name: None,
                squad_guid: None,
                interact_radius,
                auto_interact_radius,
                move_to_interact_offset,
                instance_guid,
                wield_type: (wield_type, wield_type.holster()),
                holstered: false,
                stand_animation_id,
                temporary_model_id: None,
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
        model_id: u32,
        pos: Pos,
        rot: Pos,
        chunk_size: u16,
        instance_guid: u64,
        data: Player,
        game_server: &GameServer,
    ) -> Self {
        let wield_type = wield_type_from_inventory(
            &data
                .inventory
                .equipped_items(data.inventory.active_battle_class),
            game_server,
        );
        Character {
            stats: CharacterStats {
                guid: player_guid(guid),
                model_id,
                pos,
                rot,
                possible_pos: vec![],
                chunk_size,
                scale: 1.0,
                name: Some(format!("{}", data.name)),
                squad_guid: data.squad_guid,
                character_type: CharacterType::Player(Box::new(data)),
                mount: None,
                cursor: None,
                is_spawned: true,
                physics: PhysicsState::default(),
                interact_radius: 0.0,
                auto_interact_radius: 0.0,
                move_to_interact_offset: 2.2,
                instance_guid,
                wield_type: (wield_type, wield_type.holster()),
                holstered: false,
                stand_animation_id: 0,
                temporary_model_id: None,
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

    pub fn chunk(x: f32, z: f32, chunk_size: u16) -> Chunk {
        Chunk {
            x: x.div_euclid(chunk_size as f32) as i32,
            z: z.div_euclid(chunk_size as f32) as i32,
            size: chunk_size,
        }
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
        mount_configs: &BTreeMap<u32, MountConfig>,
        item_definitions: &BTreeMap<u32, ItemDefinition>,
        customizations: &BTreeMap<u32, Customization>,
    ) -> (Vec<Broadcast>, Option<UpdatePlayerPosition>) {
        self.tickable_procedure_tracker.tick(
            &mut self.stats,
            now,
            nearby_player_guids,
            nearby_players,
            mount_configs,
            item_definitions,
            customizations,
        )
    }

    pub fn current_tickable_procedure(&self) -> Option<&String> {
        self.tickable_procedure_tracker.current_tickable_procedure()
    }

    pub fn last_procedure_change(&self) -> Instant {
        self.tickable_procedure_tracker.last_procedure_change()
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
        player_stats: &mut Player,
        nearby_player_guids: &[u32],
    ) -> WriteLockingBroadcastSupplier {
        let mut new_procedure = None;

        let character_type = self.stats.character_type.clone();

        let broadcast_supplier = match character_type {
            CharacterType::AmbientNpc(ambient_npc) => {
                let (procedure, one_shot_interact) =
                    ambient_npc.interact(self, nearby_player_guids, requester, player_stats);
                new_procedure = procedure;
                one_shot_interact
            }
            CharacterType::Door(door) => door.interact(requester),
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

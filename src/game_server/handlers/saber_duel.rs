use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    io::{Cursor, Read},
    time::{Duration, Instant},
};

use enum_iterator::all;
use packet_serialize::DeserializePacket;
use rand::{thread_rng, Rng};
use rand_distr::{Distribution, WeightedAliasIndex, WeightedIndex};
use serde::{Deserialize, Deserializer};

use crate::game_server::{
    handlers::{
        character::{
            default_spawn_animation_id, AmbientNpc, BaseNpc, Character, CharacterType,
            MinigameMatchmakingGroup, MinigameStatus, PlayerInventory,
        },
        inventory::{
            attachments_from_equipped_items, player_has_saber_equipped, wield_type_from_inventory,
        },
        minigame::{
            handle_minigame_packet_write, MinigameCountdown, MinigameRemovePlayerResult,
            MinigameStopwatch, SharedMinigameTypeData,
        },
        unique_guid::{player_guid, saber_duel_opponent_guid},
    },
    packets::{
        item::{EquipmentSlot, WieldType},
        minigame::{MinigameHeader, ScoreEntry, ScoreType},
        saber_duel::{
            SaberDuelApplyForcePower, SaberDuelBoutInfo, SaberDuelBoutStart, SaberDuelBoutTied,
            SaberDuelBoutWon, SaberDuelForcePower, SaberDuelForcePowerDefinition,
            SaberDuelForcePowerFlags, SaberDuelGameOver, SaberDuelKey, SaberDuelKeypressEvent,
            SaberDuelOpCode, SaberDuelPlayerUpdate, SaberDuelRemoveForcePower,
            SaberDuelRequestApplyForcePower, SaberDuelRoundOver, SaberDuelRoundStart,
            SaberDuelShowForcePowerDialog, SaberDuelStageData,
        },
        tunnel::TunneledPacket,
        ui::{ExecuteScriptWithIntParams, ExecuteScriptWithStringParams},
        GamePacket, Pos,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

const ROUND_END_DELAY: Duration = Duration::from_millis(2500);
const ROUND_START_DELAY: Duration = Duration::from_millis(1200);
const GAME_END_DELAY: Duration = Duration::from_millis(4000);
const MEMORY_CHALLENGE_AI_DELAY: Duration = Duration::from_millis(2000);

const fn default_ai_force_point_multiplier() -> f32 {
    1.0
}

const fn default_ai_force_cost_multiplier() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaberDuelAiForcePower {
    #[serde(default = "default_weight")]
    weight: u8,
    #[serde(default)]
    tutorial_enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaberDuelEquippableSaber {
    hilt_item_guid: u32,
    shape_item_guid: u32,
    color_item_guid: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaberDuelAi {
    name_id: u32,
    model_id: u32,
    primary_saber: Option<SaberDuelEquippableSaber>,
    secondary_saber: Option<SaberDuelEquippableSaber>,
    wield_type_override: Option<WieldType>,
    entrance_animation_id: i32,
    entrance_sound_id: Option<u32>,
    round_won_sound_id: Option<u32>,
    round_lost_sound_id: Option<u32>,
    game_won_sound_id: Option<u32>,
    game_lost_sound_id: Option<u32>,
    min_millis_per_key: u16,
    max_millis_per_key: u16,
    #[serde(deserialize_with = "deserialize_probability")]
    mistake_probability: f32,
    #[serde(default)]
    right_to_left_ai_mistake_multiplier: f32,
    #[serde(default)]
    opposite_ai_mistake_multiplier: f32,
    #[serde(default = "default_ai_force_cost_multiplier")]
    force_power_cost_multiplier: f32,
    #[serde(default = "default_ai_force_point_multiplier")]
    force_point_multiplier: f32,
    #[serde(deserialize_with = "deserialize_probability", default)]
    force_power_probability: f32,
    #[serde(default)]
    force_powers: BTreeMap<SaberDuelForcePower, SaberDuelAiForcePower>,
    #[serde(default)]
    force_power_delay_millis: u32,
}

#[derive(Clone, Debug)]
struct SaberDuelAppliedForcePower {
    force_power: SaberDuelForcePower,
    bouts_remaining: u8,
}

#[derive(Clone, Debug, Default)]
struct SaberDuelPlayerState {
    pub is_ai: bool,
    pub ready: bool,
    pub rounds_won: u8,
    pub round_points: u8,
    pub game_points_won: u16,
    pub game_points_lost: u16,
    pub win_streak: u16,
    pub longest_win_streak: u16,
    pub progress: u8,
    pub required_progress: u8,
    pub affected_by_force_powers: Vec<SaberDuelAppliedForcePower>,
    pub pending_force_power_tutorials: VecDeque<SaberDuelForcePower>,
    pub seen_force_power_tutorials: BTreeSet<SaberDuelForcePower>,
    pub force_points: u8,
    pub total_correct: u16,
    pub total_mistakes: u16,
}

impl SaberDuelPlayerState {
    pub fn is_affected_by(&self, force_power: SaberDuelForcePower) -> bool {
        self.affected_by_force_powers.iter().any(|applied_power| {
            applied_power.force_power == force_power && applied_power.bouts_remaining > 0
        })
    }

    pub fn usable_force_power<'a>(
        &self,
        force_power: SaberDuelForcePower,
        available_force_powers: &'a BTreeMap<SaberDuelForcePower, SaberDuelAvailableForcePower>,
        other_player_state: &SaberDuelPlayerState,
        ai: &SaberDuelAi,
    ) -> Option<&'a SaberDuelAvailableForcePower> {
        if other_player_state.is_affected_by(force_power) {
            return None;
        }

        self.affordable_force_power(force_power, available_force_powers, ai)
    }

    pub fn force_power_flags(
        &self,
        available_force_powers: &BTreeMap<SaberDuelForcePower, SaberDuelAvailableForcePower>,
        other_player_state: &SaberDuelPlayerState,
        ai: &SaberDuelAi,
    ) -> SaberDuelForcePowerFlags {
        SaberDuelForcePowerFlags {
            can_use_extra_key: self
                .usable_force_power(
                    SaberDuelForcePower::ExtraKey,
                    available_force_powers,
                    other_player_state,
                    ai,
                )
                .is_some(),
            can_use_right_to_left: self
                .usable_force_power(
                    SaberDuelForcePower::RightToLeft,
                    available_force_powers,
                    other_player_state,
                    ai,
                )
                .is_some(),
            can_use_opposite: self
                .usable_force_power(
                    SaberDuelForcePower::Opposite,
                    available_force_powers,
                    other_player_state,
                    ai,
                )
                .is_some(),
        }
    }

    pub fn add_force_points(&mut self, base_gain: u8, max_force_points: u8, ai: &SaberDuelAi) {
        let effective_gain = match self.is_ai {
            true => (base_gain as f32 * ai.force_point_multiplier)
                .round()
                .max(0.0) as u8,
            false => base_gain,
        };

        self.force_points = (self.force_points + effective_gain).min(max_force_points);
    }

    pub fn use_force_power(&mut self, base_cost: u8, ai: &SaberDuelAi) {
        let effective_cost = match self.is_ai {
            true => (base_cost as f32 * ai.force_power_cost_multiplier)
                .round()
                .max(0.0) as u8,
            false => base_cost,
        };

        self.force_points = self.force_points.saturating_sub(effective_cost);
    }

    pub fn apply_force_power(
        &mut self,
        force_power: SaberDuelForcePower,
        bouts_applied: u8,
        tutorial_enabled: bool,
    ) {
        self.affected_by_force_powers
            .push(SaberDuelAppliedForcePower {
                force_power,
                bouts_remaining: bouts_applied,
            });
        if tutorial_enabled && self.seen_force_power_tutorials.insert(force_power) {
            self.pending_force_power_tutorials.push_back(force_power);
        }
    }

    pub fn next_force_power_tutorial(&mut self) -> Option<SaberDuelForcePower> {
        self.pending_force_power_tutorials.pop_front()
    }

    #[must_use]
    pub fn win_bout(&mut self, points_won: u8) -> Vec<SaberDuelForcePower> {
        self.round_points = self.round_points.saturating_add(points_won);
        self.game_points_won = self.game_points_won.saturating_add(points_won.into());
        self.win_streak = self.win_streak.saturating_add(points_won.into());
        self.longest_win_streak = self.longest_win_streak.max(self.win_streak);
        self.end_bout()
    }

    #[must_use]
    pub fn tie_bout(&mut self) -> Vec<SaberDuelForcePower> {
        self.win_streak = 0;
        self.end_bout()
    }

    #[must_use]
    pub fn lose_bout(&mut self, points_lost: u8) -> Vec<SaberDuelForcePower> {
        self.game_points_lost = self.game_points_lost.saturating_add(points_lost.into());
        self.win_streak = 0;
        self.end_bout()
    }

    pub fn win_round(&mut self) {
        self.rounds_won = self.rounds_won.saturating_add(1);
    }

    pub fn reset_bout_progress(&mut self, new_required_progress: u8) {
        self.progress = 0;
        self.required_progress = new_required_progress;
    }

    pub fn reset_round_progress(&mut self) {
        self.reset_bout_progress(0);
        self.round_points = 0;
    }

    pub fn margin_of_victory(&self) -> i32 {
        (self.game_points_won as i32).saturating_sub(self.game_points_lost as i32)
    }

    #[must_use]
    pub fn increment_progress(&mut self) -> bool {
        let new_progress = self.progress.saturating_add(1).min(self.required_progress);
        self.progress = new_progress;
        self.total_correct = self.total_correct.saturating_add(1);

        new_progress >= self.required_progress
    }

    pub fn make_mistake(&mut self) {
        self.reset_bout_progress(self.required_progress);
        self.total_mistakes = self.total_mistakes.saturating_add(1);
    }

    pub fn accuracy(&self) -> f32 {
        let mistakes = Into::<f32>::into(self.total_mistakes);
        let correct = Into::<f32>::into(self.total_correct);
        let total = mistakes + correct;
        correct / total
    }

    fn end_bout(&mut self) -> Vec<SaberDuelForcePower> {
        let mut expired_powers = Vec::new();
        self.affected_by_force_powers.retain_mut(|power| {
            power.bouts_remaining = power.bouts_remaining.saturating_sub(1);
            let expired = power.bouts_remaining == 0;
            if expired {
                expired_powers.push(power.force_power);
            }

            !expired
        });

        expired_powers
    }

    fn affordable_force_power<'a>(
        &self,
        force_power: SaberDuelForcePower,
        available_force_powers: &'a BTreeMap<SaberDuelForcePower, SaberDuelAvailableForcePower>,
        ai: &SaberDuelAi,
    ) -> Option<&'a SaberDuelAvailableForcePower> {
        let definition = available_force_powers.get(&force_power)?;
        let base_cost = definition.cost;
        let effective_cost = match self.is_ai {
            true => (base_cost as f32 * ai.force_power_cost_multiplier)
                .round()
                .max(0.0) as u8,
            false => base_cost,
        };

        if self.force_points >= effective_cost {
            Some(definition)
        } else {
            None
        }
    }
}

const fn default_weight() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaberDuelAnimationPair {
    attack_animation_id: i32,
    defend_animation_id: i32,
    #[serde(default = "default_weight")]
    weight: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaberDuelAvailableForcePower {
    name_id: u32,
    small_icon_id: u32,
    icon_id: u32,
    cost: u8,
    bouts_applied: u8,
    apply_animation_id: i32,
}

#[derive(Clone, Debug)]
enum SaberDuelGameState {
    WaitingForPlayersReady {
        game_start: bool,
    },
    WaitingForForcePowers {
        timer: MinigameCountdown,
        ai_next_force_power: MinigameCountdown,
    },
    BoutActive {
        bout_time_remaining: MinigameCountdown,
        is_special_bout: bool,
        keys: Vec<SaberDuelKey>,
        ai_next_key: MinigameCountdown,
        player1_completed_time: Option<Duration>,
        player2_completed_time: Option<Duration>,
    },
    WaitingForRoundEnd {
        timer: MinigameCountdown,
    },
    WaitingForRoundStart {
        timer: MinigameCountdown,
    },
    WaitingForGameOver {
        timer: MinigameCountdown,
    },
    GameOver,
}

fn deserialize_bout_animations<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let animations: Vec<T> = Deserialize::deserialize(deserializer)?;
    if animations.is_empty() {
        Err(serde::de::Error::custom(
            "Bout animations must be non-empty",
        ))
    } else {
        Ok(animations)
    }
}

fn deserialize_probability<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let probability: f32 = Deserialize::deserialize(deserializer)?;
    if (0.0..=1.0).contains(&probability) {
        Ok(probability)
    } else {
        Err(serde::de::Error::custom(format!(
            "Probability must be between 0.0 and 1.0 (inclusive), but was {probability}"
        )))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum SaberDuelChallenge {
    #[default]
    None,
    Victory,
    Memory,
    PerfectAccuracy,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaberDuelConfig {
    camera_rot: f32,
    rounds_to_win: u8,
    points_to_win_round: u8,
    keys_per_basic_bout: u8,
    keys_per_special_bout: u8,
    first_special_bout: u8,
    special_bout_interval: u8,
    points_per_basic_bout: u8,
    points_per_special_bout: u8,
    bout_max_millis: u32,
    tie_interval_millis: u32,
    #[serde(deserialize_with = "deserialize_bout_animations")]
    basic_bout_animations: Vec<SaberDuelAnimationPair>,
    #[serde(deserialize_with = "deserialize_bout_animations")]
    special_bout_animations: Vec<SaberDuelAnimationPair>,
    establishing_animation_id: i32,
    camera_entrance_animation_id: i32,
    ai: SaberDuelAi,
    score_penalty_per_second: f32,
    max_time_score_bonus: f32,
    score_penalty_per_accuracy_pct: f32,
    max_accuracy_score_bonus: f32,
    score_per_win_streak: i32,
    score_per_point: i32,
    score_per_margin_of_victory: i32,
    game_win_bonus_score: i32,
    no_bouts_lost_bonus_score: i32,
    default_primary_saber: SaberDuelEquippableSaber,
    default_secondary_saber: Option<SaberDuelEquippableSaber>,
    #[serde(default)]
    max_force_points: u8,
    #[serde(default)]
    force_power_selection_max_millis: u32,
    #[serde(default)]
    force_points_per_bout_won: u8,
    #[serde(default)]
    force_points_per_bout_tied: u8,
    #[serde(default)]
    force_points_per_bout_lost: u8,
    #[serde(default)]
    force_powers: BTreeMap<SaberDuelForcePower, SaberDuelAvailableForcePower>,
    #[serde(default)]
    challenge: SaberDuelChallenge,
}

pub fn process_saber_duel_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let header = MinigameHeader::deserialize(cursor)?;
    handle_minigame_packet_write(
        sender,
        game_server,
        &header,
        |_, _, _, _, shared_minigame_data, _| {
            let SharedMinigameTypeData::SaberDuel { game } = &mut shared_minigame_data.data else {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!(
                        "Received Saber Duel packet from unexpected game: {}, {buffer:x?}",
                        header.sub_op_code
                    ),
                ));
            };

            match SaberDuelOpCode::try_from(header.sub_op_code) {
                Ok(op_code) => match op_code {
                    SaberDuelOpCode::PlayerReady => game.mark_player_ready(sender),
                    SaberDuelOpCode::Keypress => {
                        let event = SaberDuelKeypressEvent::deserialize(cursor)?;
                        game.handle_keypress(sender, event)
                    }
                    SaberDuelOpCode::RequestApplyForcePower => {
                        let request = SaberDuelRequestApplyForcePower::deserialize(cursor)?;
                        game.apply_force_power(sender, request.force_power)
                    }
                    _ => {
                        let mut buffer = Vec::new();
                        cursor.read_to_end(&mut buffer)?;
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::UnknownOpCode,
                            format!("Unimplemented Saber Duel op code: {op_code:?} {buffer:x?}"),
                        ))
                    }
                },
                Err(_) => {
                    let mut buffer = Vec::new();
                    cursor.read_to_end(&mut buffer)?;
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::UnknownOpCode,
                        format!(
                            "Unknown Saber Duel packet: {}, {buffer:x?}",
                            header.sub_op_code
                        ),
                    ))
                }
            }
        },
    )
}

enum SaberDuelBoutCompletion {
    NeitherPlayer,
    OnePlayer {
        bout_time_remaining_at_completion: Duration,
        player_index: u8,
    },
    BothPlayers {
        time_between_completions: Duration,
        fastest_player_index: u8,
    },
}

#[derive(Clone, Debug)]
pub struct SaberDuelGame {
    config: SaberDuelConfig,
    pos: Pos,
    basic_bout_animation_distribution: WeightedAliasIndex<u8>,
    special_bout_animation_distribution: WeightedAliasIndex<u8>,
    player1: u32,
    player2: Option<u32>,
    player_states: [SaberDuelPlayerState; 2],
    bout: u8,
    state: SaberDuelGameState,
    stopwatch: MinigameStopwatch,
    recipients: Vec<u32>,
    group: MinigameMatchmakingGroup,
}

impl SaberDuelGame {
    pub fn new(
        config: SaberDuelConfig,
        player1: u32,
        player2: Option<u32>,
        group: MinigameMatchmakingGroup,
        start_pos: Option<Pos>,
    ) -> Self {
        let mut recipients = vec![player1];
        if let Some(player2) = player2 {
            recipients.push(player2);
        }

        let basic_bout_animation_distribution = WeightedAliasIndex::new(
            config
                .basic_bout_animations
                .iter()
                .map(|animation| animation.weight)
                .collect(),
        )
        .expect("Couldn't create weighted alias index");
        let special_bout_animation_distribution = WeightedAliasIndex::new(
            config
                .special_bout_animations
                .iter()
                .map(|animation| animation.weight)
                .collect(),
        )
        .expect("Couldn't create weighted alias index");

        let mut game = SaberDuelGame {
            config,
            pos: start_pos.unwrap_or_default(),
            basic_bout_animation_distribution,
            special_bout_animation_distribution,
            player1,
            player2,
            player_states: Default::default(),
            bout: 0,
            state: SaberDuelGameState::WaitingForPlayersReady { game_start: true },
            stopwatch: MinigameStopwatch::new(None),
            recipients,
            group,
        };
        game.reset_readiness(true);
        game.player_states[1].is_ai = game.is_ai_match();

        game
    }

    pub fn characters(
        &self,
        instance_guid: u64,
        chunk_size: u16,
        game_server: &GameServer,
    ) -> Result<Vec<Character>, ProcessPacketError> {
        if !self.is_ai_match() {
            return Ok(Vec::new());
        }

        let mut items = BTreeMap::new();
        if let Some(primary_saber) = &self.config.ai.primary_saber {
            items.extend([
                (EquipmentSlot::PrimaryWeapon, primary_saber.hilt_item_guid),
                (
                    EquipmentSlot::PrimarySaberShape,
                    primary_saber.shape_item_guid,
                ),
                (
                    EquipmentSlot::PrimarySaberColor,
                    primary_saber.color_item_guid,
                ),
            ]);
        }

        if let Some(secondary_saber) = &self.config.ai.secondary_saber {
            items.extend([
                (
                    EquipmentSlot::SecondaryWeapon,
                    secondary_saber.hilt_item_guid,
                ),
                (
                    EquipmentSlot::SecondarySaberShape,
                    secondary_saber.shape_item_guid,
                ),
                (
                    EquipmentSlot::SecondarySaberColor,
                    secondary_saber.color_item_guid,
                ),
            ]);
        }
        let attachments = attachments_from_equipped_items(&items, game_server.items())
            .into_iter()
            .map(|attachment| attachment.into())
            .collect();

        let wield_type = self
            .config
            .ai
            .wield_type_override
            .unwrap_or_else(|| wield_type_from_inventory(&items, game_server));

        let opponent = Character::new(
            saber_duel_opponent_guid(self.player1),
            self.config.ai.model_id,
            self.pos,
            Pos::default(),
            chunk_size,
            1.0,
            CharacterType::AmbientNpc(AmbientNpc {
                base_npc: BaseNpc {
                    texture_alias: "".to_string(),
                    name_id: self.config.ai.name_id,
                    terrain_object_id: 0,
                    name_offset_x: 0.0,
                    name_offset_y: 0.0,
                    name_offset_z: 0.0,
                    enable_interact_popup: false,
                    interact_popup_radius: None,
                    show_name: false,
                    bounce_area_id: -1,
                    enable_gravity: true,
                    enable_tilt: false,
                    use_terrain_model: false,
                    attachments,
                    composite_effect_id: None,
                    sub_title_id: None,
                    clickable: true,
                    spawn_animation_id: default_spawn_animation_id(),
                },
                procedure_on_interact: None,
                one_shot_action_on_interact: None,
            }),
            None,
            None,
            0.0,
            0.0,
            0.0,
            instance_guid,
            wield_type,
            -1,
            HashMap::new(),
            Vec::new(),
            None,
        );

        Ok(vec![opponent])
    }

    pub fn update_gear(
        &self,
        player_inventory: &mut PlayerInventory,
        game_server: &GameServer,
    ) -> Result<(), ProcessPacketError> {
        if !player_has_saber_equipped(
            player_inventory,
            player_inventory.active_battle_class,
            game_server.items(),
        ) {
            player_inventory.equip_item_temporarily(
                EquipmentSlot::PrimaryWeapon,
                Some(self.config.default_primary_saber.hilt_item_guid),
            );
            player_inventory.equip_item_temporarily(
                EquipmentSlot::PrimarySaberShape,
                Some(self.config.default_primary_saber.shape_item_guid),
            );
            player_inventory.equip_item_temporarily(
                EquipmentSlot::PrimarySaberColor,
                Some(self.config.default_primary_saber.color_item_guid),
            );
            player_inventory.equip_item_temporarily(
                EquipmentSlot::SecondaryWeapon,
                self.config
                    .default_secondary_saber
                    .as_ref()
                    .map(|saber| saber.hilt_item_guid),
            );
            player_inventory.equip_item_temporarily(
                EquipmentSlot::SecondarySaberShape,
                self.config
                    .default_secondary_saber
                    .as_ref()
                    .map(|saber| saber.shape_item_guid),
            );
            player_inventory.equip_item_temporarily(
                EquipmentSlot::SecondarySaberColor,
                self.config
                    .default_secondary_saber
                    .as_ref()
                    .map(|saber| saber.color_item_guid),
            );
        }
        Ok(())
    }

    pub fn start(&self, sender: u32) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;

        Ok(vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: SaberDuelStageData {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: SaberDuelOpCode::StageData as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                points_to_win_round: self.config.points_to_win_round.into(),
                total_rounds: self.config.rounds_to_win.into(),
                seconds_remaining: 0,
                camera_pos: self.pos,
                camera_rot: self.config.camera_rot,
                max_combo_points: 0,
                establishing_animation_id: self.config.establishing_animation_id,
                local_player_index: player_index.into(),
                opponent_guid: match player_index {
                    0 => match self.player2 {
                        Some(opponent_guid) => player_guid(opponent_guid),
                        None => saber_duel_opponent_guid(self.player1),
                    },
                    _ => player_guid(self.player1),
                },
                opponent_entrance_animation_id: self
                    .player2
                    .map(|_| self.config.camera_entrance_animation_id)
                    .unwrap_or(self.config.ai.entrance_animation_id),
                opponent_entrance_sound_id: self
                    .player2
                    .map(|_| 0)
                    .or(self.config.ai.entrance_sound_id)
                    .unwrap_or(0),
                max_force_points: self.config.max_force_points.into(),
                paused: false,
                enable_memory_challenge: matches!(
                    self.config.challenge,
                    SaberDuelChallenge::Memory
                ),
                force_powers: self
                    .config
                    .force_powers
                    .iter()
                    .map(|(force_power, definition)| SaberDuelForcePowerDefinition {
                        force_power: *force_power,
                        name_id: definition.name_id,
                        small_icon_id: definition.small_icon_id,
                        icon_id: definition.icon_id,
                    })
                    .collect(),
            },
        })])
    }

    pub fn handle_keypress(
        &mut self,
        sender: u32,
        event: SaberDuelKeypressEvent,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;
        let SaberDuelGameState::BoutActive {
            bout_time_remaining,
            keys,
            player1_completed_time,
            player2_completed_time,
            ..
        } = &mut self.state
        else {
            return Ok(Vec::new());
        };

        if keys.is_empty() {
            return Ok(Vec::new());
        }

        if bout_time_remaining.paused() {
            return Ok(Vec::new());
        }

        let completion_time = match player_index == 0 {
            true => player1_completed_time,
            false => player2_completed_time,
        };

        let now = Instant::now();
        let time_until_bout_end = bout_time_remaining.time_until_next_event(now);

        if completion_time.is_some() || time_until_bout_end.is_zero() {
            return Ok(Vec::new());
        }

        let keypress = event.keypress;
        let player_state = &mut self.player_states[player_index as usize];

        let is_reverse = player_state.is_affected_by(SaberDuelForcePower::RightToLeft);
        let key_index = match is_reverse {
            true => player_state
                .required_progress
                .saturating_sub(player_state.progress)
                .saturating_sub(1) as usize,
            false => player_state.progress as usize,
        };

        let mut expected_key = keys[key_index];
        if player_state.is_affected_by(SaberDuelForcePower::Opposite) {
            expected_key = expected_key.opposite();
        }

        if expected_key == keypress.into() {
            if player_state.increment_progress() {
                *completion_time = Some(time_until_bout_end);
            }
        } else {
            player_state.make_mistake();
        }

        Ok(self.update_progress(player_index))
    }

    pub fn apply_force_power(
        &mut self,
        sender: u32,
        force_power: SaberDuelForcePower,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;
        let other_player_index = (player_index as usize + 1) % 2;

        self.apply_force_power_from_index(player_index, other_player_index, force_power, false)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        let is_ai_match = self.is_ai_match();
        match &mut self.state {
            SaberDuelGameState::WaitingForForcePowers {
                timer,
                ai_next_force_power,
            } => {
                if timer.time_until_next_event(now).is_zero() {
                    self.start_bout()
                } else {
                    let mut broadcasts = Vec::new();

                    if is_ai_match {
                        let ai_player_state = &self.player_states[1];
                        let other_player_state = &self.player_states[0];
                        if let Some((force_power, tutorial_enabled)) = Self::tick_ai_force_power(
                            now,
                            &self.config,
                            ai_player_state,
                            other_player_state,
                            ai_next_force_power,
                        ) {
                            broadcasts.append(
                                &mut self
                                    .apply_force_power_from_index(
                                        1,
                                        0,
                                        force_power,
                                        tutorial_enabled,
                                    )
                                    .expect("Chose force power that Saber Duel AI can't use"),
                            );
                        }
                    }

                    broadcasts
                }
            }
            SaberDuelGameState::WaitingForRoundEnd { timer } => {
                if timer.time_until_next_event(now).is_zero() {
                    self.state = SaberDuelGameState::WaitingForRoundStart {
                        timer: MinigameCountdown::new_with_event(ROUND_START_DELAY),
                    };
                    self.player_states
                        .iter_mut()
                        .for_each(|player_state| player_state.reset_round_progress());
                    vec![Broadcast::Multi(
                        self.recipients.clone(),
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: SaberDuelRoundStart {
                                minigame_header: MinigameHeader {
                                    stage_guid: self.group.stage_guid,
                                    sub_op_code: SaberDuelOpCode::RoundStart as i32,
                                    stage_group_guid: self.group.stage_group_guid,
                                },
                            },
                        })],
                    )]
                } else {
                    Vec::new()
                }
            }
            SaberDuelGameState::WaitingForRoundStart { timer } => {
                if timer.time_until_next_event(now).is_zero() {
                    self.prepare_bout()
                } else {
                    Vec::new()
                }
            }
            SaberDuelGameState::WaitingForGameOver { timer } => {
                if timer.time_until_next_event(now).is_zero() {
                    self.state = SaberDuelGameState::GameOver;
                    vec![Broadcast::Multi(
                        self.recipients.clone(),
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: ExecuteScriptWithStringParams {
                                script_name: "Ui.QuitMiniGame".to_string(),
                                params: Vec::new(),
                            },
                        })],
                    )]
                } else {
                    Vec::new()
                }
            }
            SaberDuelGameState::BoutActive {
                bout_time_remaining,
                is_special_bout,
                ai_next_key,
                player1_completed_time,
                player2_completed_time,
                ..
            } => {
                let bout_time_remaining_now = bout_time_remaining.time_until_next_event(now);
                let is_special_bout = *is_special_bout;

                let bout_completion = match (&player1_completed_time, &player2_completed_time) {
                    (None, None) => SaberDuelBoutCompletion::NeitherPlayer,
                    (None, Some(player2_time)) => SaberDuelBoutCompletion::OnePlayer {
                        bout_time_remaining_at_completion: *player2_time,
                        player_index: 1,
                    },
                    (Some(player1_time), None) => SaberDuelBoutCompletion::OnePlayer {
                        bout_time_remaining_at_completion: *player1_time,
                        player_index: 0,
                    },
                    (Some(player1_time), Some(player2_time)) => {
                        // The player who finished with the most time until the bout's end is the winner
                        let fastest_player_index = match player1_time > player2_time {
                            true => 0,
                            false => 1,
                        };

                        SaberDuelBoutCompletion::BothPlayers {
                            time_between_completions: player1_time.abs_diff(*player2_time),
                            fastest_player_index,
                        }
                    }
                };

                let tie_interval = Duration::from_millis(self.config.tie_interval_millis.into());

                match bout_completion {
                    SaberDuelBoutCompletion::NeitherPlayer => {
                        if bout_time_remaining_now.is_zero() {
                            return self.tie_bout();
                        }
                    }
                    SaberDuelBoutCompletion::OnePlayer {
                        bout_time_remaining_at_completion,
                        player_index,
                    } => {
                        let time_since_completion = bout_time_remaining_at_completion
                            .saturating_sub(bout_time_remaining_now);
                        if time_since_completion > tie_interval || bout_time_remaining_now.is_zero()
                        {
                            return self.win_bout(player_index, is_special_bout);
                        }
                    }
                    SaberDuelBoutCompletion::BothPlayers {
                        time_between_completions,
                        fastest_player_index,
                    } => {
                        if time_between_completions > tie_interval {
                            return self.win_bout(fastest_player_index, is_special_bout);
                        }

                        return self.tie_bout();
                    }
                }

                if is_ai_match
                    && Self::tick_ai_keypress(
                        now,
                        &self.config,
                        &mut self.player_states[1],
                        ai_next_key,
                        bout_time_remaining,
                        player2_completed_time,
                    )
                {
                    self.update_progress(1)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    pub fn pause_or_resume(
        &mut self,
        player: u32,
        pause: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        self.player_index(player)?;

        if !self.is_ai_match() {
            return Ok(Vec::new());
        }

        match &mut self.state {
            SaberDuelGameState::WaitingForForcePowers {
                timer,
                ai_next_force_power,
            } => {
                timer.pause_or_resume(pause);
                ai_next_force_power.pause_or_resume(pause);
            }
            SaberDuelGameState::BoutActive {
                bout_time_remaining,
                ai_next_key,
                ..
            } => {
                bout_time_remaining.pause_or_resume(pause);
                ai_next_key.pause_or_resume(pause);
            }
            SaberDuelGameState::WaitingForRoundEnd { timer } => timer.pause_or_resume(pause),
            SaberDuelGameState::WaitingForRoundStart { timer } => timer.pause_or_resume(pause),
            SaberDuelGameState::WaitingForGameOver { timer } => timer.pause_or_resume(pause),
            _ => {}
        }

        // We don't want to unpause the stopwatch if we've paused it at the start or end of the duel
        if !matches!(
            self.state,
            SaberDuelGameState::WaitingForPlayersReady { game_start: true }
                | SaberDuelGameState::WaitingForGameOver { .. }
                | SaberDuelGameState::GameOver
        ) {
            self.stopwatch.pause_or_resume(pause);
        }

        Ok(Vec::new())
    }

    pub fn remove_player(
        &self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<MinigameRemovePlayerResult, ProcessPacketError> {
        let player_index = self.player_index(player)? as usize;

        let player_state = &self.player_states[player_index];

        let beat_opponent = Self::has_player_beat_opponent(&self.config, player_state);
        minigame_status
            .win_status
            .set_won(Self::has_player_won_game(&self.config, player_state));

        let mut total_score: i32 = 0;
        if beat_opponent {
            total_score = total_score.saturating_add(self.config.game_win_bonus_score);

            if player_state.game_points_lost == 0 {
                total_score = total_score.saturating_add(self.config.no_bouts_lost_bonus_score);
            }
        }

        total_score = total_score.saturating_add(
            player_state
                .margin_of_victory()
                .saturating_mul(self.config.score_per_margin_of_victory),
        );
        total_score = total_score.saturating_add(
            (player_state.game_points_won as i32).saturating_mul(self.config.score_per_point),
        );

        // Time
        let duel_seconds = i16::try_from(self.stopwatch.elapsed().as_secs()).unwrap_or(i16::MAX);
        let time_bonus = match beat_opponent {
            true => (self.config.max_time_score_bonus
                - Into::<f32>::into(duel_seconds) * self.config.score_penalty_per_second)
                .round()
                .max(0.0) as i32,
            false => 0,
        };
        total_score = total_score.saturating_add(time_bonus);
        minigame_status.score_entries.push(ScoreEntry {
            entry_text: "ld_TimeMod".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Time,
            score_count: duel_seconds as i32,
            score_max: 0,
            score_points: 0,
        });
        if time_bonus > 0 {
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "ld_timeBonus".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: time_bonus,
                score_max: 0,
                score_points: 0,
            });
        }

        // Accuracy
        let accuracy = self.player_states[player_index].accuracy() * 100.0;
        let accuracy_bonus = (self.config.max_accuracy_score_bonus
            - self.config.score_penalty_per_accuracy_pct * (100.0 - accuracy))
            .round()
            .max(0.0) as i32;
        total_score = total_score.saturating_add(accuracy_bonus);
        minigame_status.score_entries.push(ScoreEntry {
            entry_text: "ld_accuracy".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Counter,
            score_count: accuracy.floor() as i32,
            score_max: 0,
            score_points: 0,
        });
        if accuracy_bonus > 0 {
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "ld_accuracyBonus".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: accuracy_bonus,
                score_max: 0,
                score_points: 0,
            });
        }

        // Win streak
        let win_streak = self.player_states[player_index].longest_win_streak as i32;
        let win_streak_bonus = win_streak.saturating_mul(self.config.score_per_win_streak);
        total_score = total_score.saturating_add(win_streak_bonus);
        minigame_status.score_entries.push(ScoreEntry {
            entry_text: "ld_longestWinStreak".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Counter,
            score_count: win_streak,
            score_max: 0,
            score_points: 0,
        });
        if win_streak_bonus > 0 {
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "ld_streakBonus".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: win_streak_bonus,
                score_max: 0,
                score_points: 0,
            });
        }

        minigame_status.total_score = total_score.max(0);
        minigame_status.score_entries.push(ScoreEntry {
            entry_text: "".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Total,
            score_count: minigame_status.total_score,
            score_max: 0,
            score_points: 0,
        });

        Ok(MinigameRemovePlayerResult {
            broadcasts: Vec::new(),
            characters_to_remove: match self.is_ai_match() {
                true => vec![saber_duel_opponent_guid(self.player1)],
                false => Vec::new(),
            },
            end_game_for_all: true,
        })
    }

    pub fn mark_player_ready(&mut self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)? as usize;

        let SaberDuelGameState::WaitingForPlayersReady { game_start } = &self.state else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {sender} sent a ready packet for Saber Duel, but the game isn't waiting for readiness ({self:?})")
            ));
        };

        if self.player_states[player_index].ready {
            return Ok(Vec::new());
        }
        self.player_states[player_index].ready = true;

        if !self.player_states[0].ready || !self.player_states[1].ready {
            return Ok(Vec::new());
        }

        let mut broadcasts = Vec::new();

        if *game_start {
            self.stopwatch.pause_or_resume(false);
            broadcasts.push(Broadcast::Multi(
                self.recipients.clone(),
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: SaberDuelRoundStart {
                        minigame_header: MinigameHeader {
                            stage_guid: self.group.stage_guid,
                            sub_op_code: SaberDuelOpCode::RoundStart as i32,
                            stage_group_guid: self.group.stage_group_guid,
                        },
                    },
                })],
            ));
        }

        let leader_index =
            match self.player_states[0].round_points > self.player_states[1].round_points {
                true => 0u8,
                false => 1u8,
            };
        let leader_state = &mut self.player_states[leader_index as usize];

        if leader_state.round_points >= self.config.points_to_win_round {
            leader_state.win_round();
            self.bout = 0;

            if Self::has_player_beat_opponent(&self.config, leader_state) {
                broadcasts.append(&mut self.prepare_game_end(leader_index));
            } else {
                broadcasts.append(&mut self.prepare_round_end(leader_index));
            }
        } else {
            broadcasts.append(&mut self.prepare_bout());
        }

        Ok(broadcasts)
    }

    fn is_ai_match(&self) -> bool {
        self.player2.is_none()
    }

    fn player_index(&self, sender: u32) -> Result<u8, ProcessPacketError> {
        if sender == self.player1 {
            Ok(0)
        } else if Some(sender) == self.player2 {
            Ok(1)
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {sender} sent a packet for Saber Duel, but they aren't one of the game's players ({self:?})")
            ))
        }
    }

    fn prepare_bout(&mut self) -> Vec<Broadcast> {
        let mut broadcasts = vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelBoutInfo {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutInfo as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    max_bout_time_millis: self.config.bout_max_millis,
                    is_combo_bout: false,
                    force_points_by_player_index: vec![
                        self.player_states[0].force_points.into(),
                        self.player_states[1].force_points.into(),
                    ],
                },
            })],
        )];

        let mut show_force_power_dialog = false;

        let player1_flags = self.player_states[0].force_power_flags(
            &self.config.force_powers,
            &self.player_states[1],
            &self.config.ai,
        );
        show_force_power_dialog |= player1_flags.can_use_any();

        let player2_flags = self.player_states[1].force_power_flags(
            &self.config.force_powers,
            &self.player_states[0],
            &self.config.ai,
        );
        show_force_power_dialog |= player2_flags.can_use_any();

        if show_force_power_dialog {
            broadcasts.append(&mut self.show_force_power_dialog(player1_flags, player2_flags));
        } else {
            broadcasts.append(&mut self.start_bout());
        }

        broadcasts
    }

    fn show_force_power_dialog(
        &mut self,
        player1_flags: SaberDuelForcePowerFlags,
        player2_flags: SaberDuelForcePowerFlags,
    ) -> Vec<Broadcast> {
        let mut broadcasts = Vec::new();

        broadcasts.push(Broadcast::Single(
            self.player1,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelShowForcePowerDialog {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::ShowForcePowerDialog as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    flags: player1_flags,
                },
            })],
        ));

        if let Some(player2) = self.player2 {
            broadcasts.push(Broadcast::Single(
                player2,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: SaberDuelShowForcePowerDialog {
                        minigame_header: MinigameHeader {
                            stage_guid: self.group.stage_guid,
                            sub_op_code: SaberDuelOpCode::ShowForcePowerDialog as i32,
                            stage_group_guid: self.group.stage_group_guid,
                        },
                        flags: player2_flags,
                    },
                })],
            ));
        }

        self.state = SaberDuelGameState::WaitingForForcePowers {
            timer: MinigameCountdown::new_with_event(Duration::from_millis(
                self.config.force_power_selection_max_millis.into(),
            )),
            ai_next_force_power: MinigameCountdown::new_with_event(Duration::from_millis(
                self.config.ai.force_power_delay_millis.into(),
            )),
        };

        broadcasts
    }

    fn start_bout(&mut self) -> Vec<Broadcast> {
        self.bout = self.bout.saturating_add(1);
        let is_special_bout = self.bout >= self.config.first_special_bout
            && (self.bout - self.config.first_special_bout)
                .is_multiple_of(self.config.special_bout_interval);

        let base_sequence_len = if is_special_bout {
            self.config.keys_per_special_bout
        } else {
            self.config.keys_per_basic_bout
        };

        // Add a key for the extra key force power
        let extra_key_sequence_len = base_sequence_len.saturating_add(1);

        let mut keys: Vec<SaberDuelKey> = Vec::new();
        for _ in 0..extra_key_sequence_len {
            keys.push(thread_rng().gen());
        }

        let mut time_until_first_ai_key = SaberDuelGame::next_ai_key_duration(&self.config);
        if matches!(self.config.challenge, SaberDuelChallenge::Memory) {
            time_until_first_ai_key =
                time_until_first_ai_key.saturating_add(MEMORY_CHALLENGE_AI_DELAY);
        }

        self.state = SaberDuelGameState::BoutActive {
            bout_time_remaining: MinigameCountdown::new_with_event(Duration::from_millis(
                self.config.bout_max_millis.into(),
            )),
            is_special_bout,
            keys: keys.clone(),
            ai_next_key: MinigameCountdown::new_with_event(time_until_first_ai_key),
            player1_completed_time: None,
            player2_completed_time: None,
        };

        let player1_keys = match self.player_states[0].is_affected_by(SaberDuelForcePower::ExtraKey)
        {
            true => extra_key_sequence_len,
            false => base_sequence_len,
        };
        let player2_keys = match self.player_states[1].is_affected_by(SaberDuelForcePower::ExtraKey)
        {
            true => extra_key_sequence_len,
            false => base_sequence_len,
        };

        self.player_states[0].reset_bout_progress(player1_keys);
        self.player_states[1].reset_bout_progress(player2_keys);

        let mut broadcasts = vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelBoutStart {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutStart as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    keys,
                    num_keys_by_player_index: vec![player1_keys.into(), player2_keys.into()],
                },
            })],
        )];

        broadcasts.append(&mut self.show_next_force_tutorial(self.player1, 0));
        if let Some(player2) = self.player2 {
            broadcasts.append(&mut self.show_next_force_tutorial(player2, 1));
        }

        broadcasts
    }

    fn next_ai_key_duration(config: &SaberDuelConfig) -> Duration {
        let millis =
            thread_rng().gen_range(config.ai.min_millis_per_key..=config.ai.max_millis_per_key);
        Duration::from_millis(millis.into())
    }

    #[must_use]
    fn tick_ai_keypress(
        now: Instant,
        config: &SaberDuelConfig,
        player_state: &mut SaberDuelPlayerState,
        ai_next_key: &mut MinigameCountdown,
        bout_time_remaining: &mut MinigameCountdown,
        bout_completed_time: &mut Option<Duration>,
    ) -> bool {
        if ai_next_key.time_until_next_event(now) > Duration::ZERO {
            return false;
        }

        let mut mistake_probability: f32 = config.ai.mistake_probability;
        if player_state.is_affected_by(SaberDuelForcePower::RightToLeft) {
            mistake_probability *= config.ai.right_to_left_ai_mistake_multiplier;
        }

        if player_state.is_affected_by(SaberDuelForcePower::Opposite) {
            mistake_probability *= config.ai.opposite_ai_mistake_multiplier;
        }

        mistake_probability = mistake_probability.clamp(0.0, 1.0);

        if mistake_probability.is_nan() {
            mistake_probability = 0.0;
        }

        let make_mistake = thread_rng().gen_bool(mistake_probability.into());
        if make_mistake {
            player_state.make_mistake();
        } else if player_state.increment_progress() && bout_completed_time.is_none() {
            *bout_completed_time = Some(bout_time_remaining.time_until_next_event(now));
        }
        ai_next_key.schedule_event(SaberDuelGame::next_ai_key_duration(config), now);

        true
    }

    fn update_progress(&self, player_index: u8) -> Vec<Broadcast> {
        let player_state = &self.player_states[player_index as usize];

        vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelPlayerUpdate {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::PlayerUpdate as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    player_index: player_index.into(),
                    progress: player_state.progress.into(),
                },
            })],
        )]
    }

    fn win_bout(&mut self, winner_index: u8, is_special_bout: bool) -> Vec<Broadcast> {
        self.reset_readiness(false);
        let points_won = match is_special_bout {
            true => self.config.points_per_special_bout,
            false => self.config.points_per_basic_bout,
        };

        let loser_index = (winner_index + 1) % 2;
        let loser_state = &mut self.player_states[loser_index as usize];
        loser_state.add_force_points(
            self.config.force_points_per_bout_lost,
            self.config.max_force_points,
            &self.config.ai,
        );
        let loser_cleared_powers = loser_state.lose_bout(points_won);
        let mut broadcasts = self.clear_force_powers(winner_index, loser_cleared_powers);

        let winner_state = &mut self.player_states[winner_index as usize];
        winner_state.add_force_points(
            self.config.force_points_per_bout_won,
            self.config.max_force_points,
            &self.config.ai,
        );
        let winner_cleared_powers = winner_state.win_bout(points_won);
        broadcasts.append(&mut self.clear_force_powers(loser_index, winner_cleared_powers));

        let rng = &mut thread_rng();
        let animation_pair = match is_special_bout {
            true => {
                &self.config.special_bout_animations
                    [self.special_bout_animation_distribution.sample(rng)]
            }
            false => {
                &self.config.basic_bout_animations
                    [self.basic_bout_animation_distribution.sample(rng)]
            }
        };

        broadcasts.push(Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelBoutWon {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutWon as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    winner_index: winner_index.into(),
                    points_won: points_won.into(),
                    winner_animation_id: animation_pair.attack_animation_id,
                    loser_animation_id: animation_pair.defend_animation_id,
                },
            })],
        ));

        broadcasts
    }

    fn tie_bout(&mut self) -> Vec<Broadcast> {
        self.reset_readiness(false);

        let mut broadcasts = Vec::new();
        for player_index in 0..2 {
            let player_state = &mut self.player_states[player_index as usize];
            let cleared_powers = player_state.tie_bout();
            player_state.add_force_points(
                self.config.force_points_per_bout_tied,
                self.config.max_force_points,
                &self.config.ai,
            );
            broadcasts.append(&mut self.clear_force_powers((player_index + 1) % 2, cleared_powers));
        }

        broadcasts.push(Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelBoutTied {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutTied as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                },
            })],
        ));

        broadcasts
    }

    fn reset_readiness(&mut self, game_start: bool) {
        self.state = SaberDuelGameState::WaitingForPlayersReady { game_start };
        self.player_states[0].ready = false;
        self.player_states[1].ready = self.is_ai_match();
    }

    fn prepare_round_end(&mut self, leader_index: u8) -> Vec<Broadcast> {
        self.state = SaberDuelGameState::WaitingForRoundEnd {
            timer: MinigameCountdown::new_with_event(ROUND_END_DELAY),
        };
        vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelRoundOver {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::RoundOver as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    winner_index: leader_index.into(),
                    sound_id: match self.is_ai_match() {
                        true => match leader_index == 0 {
                            true => self.config.ai.round_lost_sound_id.unwrap_or(0),
                            false => self.config.ai.round_won_sound_id.unwrap_or(0),
                        },
                        false => 0,
                    },
                },
            })],
        )]
    }

    fn has_player_beat_opponent(
        config: &SaberDuelConfig,
        player_state: &SaberDuelPlayerState,
    ) -> bool {
        player_state.rounds_won >= config.rounds_to_win
    }

    fn has_player_failed_challenge(
        config: &SaberDuelConfig,
        player_state: &SaberDuelPlayerState,
    ) -> bool {
        let is_challenge = !matches!(config.challenge, SaberDuelChallenge::None);
        let is_accuracy_challenge = matches!(config.challenge, SaberDuelChallenge::PerfectAccuracy);
        let beat_opponent = Self::has_player_beat_opponent(config, player_state);
        let made_mistake = player_state.total_mistakes > 0;

        is_challenge && (!beat_opponent || (is_accuracy_challenge && made_mistake))
    }

    fn has_player_won_game(config: &SaberDuelConfig, player_state: &SaberDuelPlayerState) -> bool {
        Self::has_player_beat_opponent(config, player_state)
            && !Self::has_player_failed_challenge(config, player_state)
    }

    fn prepare_game_end(&mut self, leader_index: u8) -> Vec<Broadcast> {
        self.state = SaberDuelGameState::WaitingForGameOver {
            timer: MinigameCountdown::new_with_event(GAME_END_DELAY),
        };
        self.stopwatch.pause_or_resume(true);

        let sound_id = match self.is_ai_match() {
            true => match leader_index == 0 {
                true => self.config.ai.game_lost_sound_id.unwrap_or(0),
                false => self.config.ai.game_won_sound_id.unwrap_or(0),
            },
            false => 0,
        };

        let mut broadcasts = vec![Broadcast::Single(
            self.player1,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelGameOver {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::GameOver as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    winner_index: leader_index.into(),
                    sound_id,
                    round_lost: false,
                    challenge_failed: Self::has_player_failed_challenge(
                        &self.config,
                        &self.player_states[0],
                    ),
                },
            })],
        )];

        if let Some(player2) = self.player2 {
            broadcasts.push(Broadcast::Single(
                player2,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: SaberDuelGameOver {
                        minigame_header: MinigameHeader {
                            stage_guid: self.group.stage_guid,
                            sub_op_code: SaberDuelOpCode::GameOver as i32,
                            stage_group_guid: self.group.stage_group_guid,
                        },
                        winner_index: leader_index.into(),
                        sound_id,
                        round_lost: false,
                        challenge_failed: Self::has_player_failed_challenge(
                            &self.config,
                            &self.player_states[1],
                        ),
                    },
                })],
            ));
        }

        broadcasts
    }

    fn apply_force_power_from_index(
        &mut self,
        player_index: u8,
        other_player_index: usize,
        force_power: SaberDuelForcePower,
        tutorial_enabled: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let is_ai_player = self.is_ai_match() && player_index == 1;

        let player_state = &self.player_states[player_index as usize];
        let other_player_state = &self.player_states[other_player_index];

        let definition = player_state.usable_force_power(
            force_power,
            &self.config.force_powers,
            other_player_state,
            &self.config.ai,
        );

        let Some(definition) = definition else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player index {player_index} tried to apply power {:?} Saber Duel, but they can't use it ({self:?})", 
                    force_power
                )
            ));
        };

        let other_player_state = &mut self.player_states[other_player_index];
        other_player_state.apply_force_power(
            force_power,
            definition.bouts_applied,
            tutorial_enabled,
        );

        let player_state = &mut self.player_states[player_index as usize];
        player_state.use_force_power(definition.cost, &self.config.ai);

        let player_state = &self.player_states[player_index as usize];
        let other_player_state = &self.player_states[other_player_index];
        let flags = player_state.force_power_flags(
            &self.config.force_powers,
            other_player_state,
            &self.config.ai,
        );

        Ok(vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelApplyForcePower {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::ApplyForcePower as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    used_by_player_index: player_index.into(),
                    force_power,
                    bouts_remaining: definition.bouts_applied.into(),
                    new_force_points: player_state.force_points.into(),
                    animation_id: definition.apply_animation_id,
                    flags,
                },
            })],
        )])
    }

    fn show_next_force_tutorial(&mut self, player: u32, player_index: usize) -> Vec<Broadcast> {
        match self.player_states[player_index].next_force_power_tutorial() {
            Some(force_power) => vec![Broadcast::Single(
                player,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: ExecuteScriptWithIntParams {
                        script_name: "UIGlobal.LightsaberDuelShowForcePowerTutorial".to_string(),
                        params: vec![force_power.into()],
                    },
                })],
            )],
            None => Vec::new(),
        }
    }

    fn tick_ai_force_power(
        now: Instant,
        config: &SaberDuelConfig,
        ai_player_state: &SaberDuelPlayerState,
        other_player_state: &SaberDuelPlayerState,
        ai_next_force_power: &mut MinigameCountdown,
    ) -> Option<(SaberDuelForcePower, bool)> {
        if ai_next_force_power.time_until_next_event(now) > Duration::ZERO {
            return None;
        }

        ai_next_force_power.schedule_event(
            Duration::from_millis(config.ai.force_power_delay_millis.into()),
            now,
        );

        let mut rng = thread_rng();
        if !rng.gen_bool(config.ai.force_power_probability.into()) {
            return None;
        }

        let weights: Vec<(SaberDuelForcePower, u8, bool)> = all::<SaberDuelForcePower>()
            .map(|force_power| {
                let (weight, tutorial_enabled) = match ai_player_state.usable_force_power(
                    force_power,
                    &config.force_powers,
                    other_player_state,
                    &config.ai,
                ) {
                    Some(_) if !other_player_state.is_affected_by(force_power) => config
                        .ai
                        .force_powers
                        .get(&force_power)
                        .map(|definition| (definition.weight, definition.tutorial_enabled))
                        .unwrap_or((0, false)),
                    _ => (0, false),
                };

                (force_power, weight, tutorial_enabled)
            })
            .collect();

        let Ok(weighted_index) = WeightedIndex::new(weights.iter().map(|weight| weight.1)) else {
            return None;
        };

        let index = weighted_index.sample(&mut rng);
        Some((weights[index].0, weights[index].2))
    }

    fn clear_force_powers(
        &self,
        used_by_player_index: u8,
        force_powers: Vec<SaberDuelForcePower>,
    ) -> Vec<Broadcast> {
        let mut packets = Vec::new();
        for force_power in force_powers.into_iter() {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelRemoveForcePower {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: SaberDuelOpCode::RemoveForcePower as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    used_by_player_index: used_by_player_index.into(),
                    force_power,
                },
            }));
        }

        vec![Broadcast::Multi(self.recipients.clone(), packets)]
    }
}

use std::{
    io::{Cursor, Read},
    time::{Duration, Instant},
};

use packet_serialize::DeserializePacket;
use rand::{thread_rng, Rng};
use rand_distr::{Distribution, WeightedAliasIndex};
use serde::{Deserialize, Deserializer};

use crate::game_server::{
    handlers::{
        character::{Character, MinigameStatus},
        minigame::{handle_minigame_packet_write, MinigameTimer, SharedMinigameTypeData},
        unique_guid::{player_guid, saber_duel_opponent_guid},
    },
    packets::{
        client_update::Position,
        item::{BaseAttachmentGroup, WieldType},
        minigame::MinigameHeader,
        player_update::{AddNpc, Hostility, Icon, RemoveStandard},
        saber_duel::{
            SaberDuelBoutInfo, SaberDuelBoutStart, SaberDuelBoutTied, SaberDuelBoutWon,
            SaberDuelForcePower, SaberDuelForcePowerDefinition, SaberDuelForcePowerFlags,
            SaberDuelGameStart, SaberDuelKey, SaberDuelKeypressEvent, SaberDuelOpCode,
            SaberDuelPlayerUpdate, SaberDuelRoundOver, SaberDuelShowForcePowerDialog,
            SaberDuelStageData,
        },
        tunnel::TunneledPacket,
        GamePacket, Pos, Target,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

#[derive(Clone, Debug, Deserialize)]
struct SaberDuelAiForcePower {
    force_power: SaberDuelForcePower,
    weight: u8,
}

#[derive(Clone, Debug, Deserialize)]
struct SaberDuelAi {
    name_id: u32,
    model_id: u32,
    wield_type: WieldType,
    entrance_animation_id: i32,
    entrance_sound_id: u32,
    bout_won_sound_id: u32,
    bout_lost_sound_id: u32,
    game_won_sound_id: u32,
    game_lost_sound_id: u32,
    millis_per_key: u16,
    #[serde(deserialize_with = "deserialize_probability")]
    mistake_probability: f32,
    right_to_left_ai_mistake_multiplier: f32,
    opposite_ai_mistake_multiplier: f32,
    #[serde(deserialize_with = "deserialize_probability")]
    force_power_probability: f32,
    force_powers: Vec<SaberDuelAiForcePower>,
}

impl Default for SaberDuelAi {
    fn default() -> Self {
        Self {
            name_id: Default::default(),
            model_id: Default::default(),
            wield_type: WieldType::SingleSaber,
            entrance_animation_id: Default::default(),
            entrance_sound_id: Default::default(),
            bout_won_sound_id: Default::default(),
            bout_lost_sound_id: Default::default(),
            game_won_sound_id: Default::default(),
            game_lost_sound_id: Default::default(),
            millis_per_key: Default::default(),
            mistake_probability: Default::default(),
            right_to_left_ai_mistake_multiplier: Default::default(),
            opposite_ai_mistake_multiplier: Default::default(),
            force_power_probability: Default::default(),
            force_powers: Default::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct SaberDuelAppliedForcePower {
    force_power: SaberDuelForcePower,
    bouts_remaining: u8,
}

#[derive(Clone, Debug, Default)]
struct SaberDuelPlayerState {
    pub ready: bool,
    pub rounds_won: u8,
    pub bouts_won: u8,
    pub progress: u8,
    pub required_progress: u8,
    pub affected_by_force_powers: Vec<SaberDuelAppliedForcePower>,
    pub saw_force_power_tutorial: bool,
    pub force_points: u8,
    pub total_correct: u32,
    pub total_mistakes: u32,
}

impl SaberDuelPlayerState {
    pub fn is_affected_by(&self, force_power: SaberDuelForcePower) -> bool {
        self.affected_by_force_powers.iter().any(|applied_power| {
            applied_power.force_power == force_power && applied_power.bouts_remaining > 0
        })
    }

    pub fn can_afford(
        &self,
        force_power: SaberDuelForcePower,
        available_force_powers: &[SaberDuelAvailableForcePower],
    ) -> bool {
        available_force_powers.iter().any(|power| {
            self.force_points >= power.cost && power.definition.force_power == force_power
        })
    }

    pub fn force_power_flags(
        &self,
        available_force_powers: &[SaberDuelAvailableForcePower],
    ) -> SaberDuelForcePowerFlags {
        SaberDuelForcePowerFlags {
            can_use_extra_key: self
                .can_afford(SaberDuelForcePower::ExtraKey, available_force_powers),
            can_use_right_to_left: self
                .can_afford(SaberDuelForcePower::RightToLeft, available_force_powers),
            can_use_opposite: self
                .can_afford(SaberDuelForcePower::Opposite, available_force_powers),
        }
    }

    pub fn add_force_points(&mut self, force_points: u8, max_force_points: u8) {
        self.force_points = self
            .force_points
            .saturating_add(force_points)
            .min(max_force_points);
    }

    pub fn reset_bout_progress(&mut self, new_required_progress: u8) {
        self.progress = 0;
        self.required_progress = new_required_progress;
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
}

const fn default_weight() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize)]
struct SaberDuelAnimationPair {
    attack_animation_id: i32,
    defend_animation_id: i32,
    #[serde(default = "default_weight")]
    weight: u8,
}

#[derive(Clone, Debug, Deserialize)]
struct SaberDuelAvailableForcePower {
    #[serde(flatten)]
    definition: SaberDuelForcePowerDefinition,
    cost: u8,
}

#[derive(Clone, Debug)]
enum SaberDuelGameState {
    WaitingForPlayersReady {
        game_start: bool,
    },
    WaitingForForcePowers {
        timer: MinigameTimer,
    },
    BoutActive {
        bout_time_remaining: MinigameTimer,
        is_long_bout: bool,
        keys: Vec<SaberDuelKey>,
        ai_next_key: MinigameTimer,
        player1_completed_time: Option<Instant>,
        player2_completed_time: Option<Instant>,
    },
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

#[derive(Clone, Debug, Deserialize)]
pub struct SaberDuelConfig {
    pos: Pos,
    camera_rot: f32,
    rounds_to_win: u8,
    bouts_to_win_round: u8,
    keys_per_short_bout: u8,
    keys_per_long_bout: u8,
    first_long_bout: u8,
    long_bout_interval: u8,
    bout_max_millis: u32,
    tie_interval_millis: u32,
    #[serde(deserialize_with = "deserialize_bout_animations")]
    short_bout_animations: Vec<SaberDuelAnimationPair>,
    #[serde(deserialize_with = "deserialize_bout_animations")]
    long_bout_animations: Vec<SaberDuelAnimationPair>,
    establishing_animation_id: i32,
    player_entrance_animation_id: i32,
    ai: SaberDuelAi,
    max_force_points: u8,
    force_power_selection_max_millis: u32,
    force_points_per_bout_won: u8,
    force_points_per_bout_tied: u8,
    force_points_per_bout_lost: u8,
    force_powers: Vec<SaberDuelAvailableForcePower>,
    force_power_tutorial: Option<SaberDuelForcePower>,
    memory_challenge: bool,
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
                    SaberDuelOpCode::RequestApplyForcePower => Ok(Vec::new()),
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
        time_since_completion: Duration,
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
    short_bout_animation_distribution: WeightedAliasIndex<u8>,
    long_bout_animation_distribution: WeightedAliasIndex<u8>,
    player1: u32,
    player2: Option<u32>,
    player_states: [SaberDuelPlayerState; 2],
    bout: u8,
    state: SaberDuelGameState,
    recipients: Vec<u32>,
    stage_guid: i32,
    stage_group_guid: i32,
}

impl SaberDuelGame {
    pub fn new(
        config: SaberDuelConfig,
        player1: u32,
        player2: Option<u32>,
        stage_guid: i32,
        stage_group_guid: i32,
    ) -> Self {
        let mut recipients = vec![player1];
        if let Some(player2) = player2 {
            recipients.push(player2);
        }

        let short_bout_animation_distribution = WeightedAliasIndex::new(
            config
                .short_bout_animations
                .iter()
                .map(|animation| animation.weight)
                .collect(),
        )
        .expect("Couldn't create weighted alias index");
        let long_bout_animation_distribution = WeightedAliasIndex::new(
            config
                .long_bout_animations
                .iter()
                .map(|animation| animation.weight)
                .collect(),
        )
        .expect("Couldn't create weighted alias index");

        let mut game = SaberDuelGame {
            config,
            short_bout_animation_distribution,
            long_bout_animation_distribution,
            player1,
            player2,
            player_states: Default::default(),
            bout: 0,
            state: SaberDuelGameState::WaitingForPlayersReady { game_start: true },
            recipients,
            stage_guid,
            stage_group_guid,
        };
        game.reset_readiness(true);

        game
    }

    pub fn start(&self, sender: u32) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;

        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: Position {
                player_pos: self.config.pos,
                rot: Pos::default(),
                is_teleport: true,
                unknown2: true,
            },
        })];

        if self.player2.is_none() {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AddNpc {
                    guid: saber_duel_opponent_guid(self.player1),
                    name_id: self.config.ai.name_id,
                    model_id: self.config.ai.model_id,
                    unknown3: false,
                    chat_text_color: Character::DEFAULT_CHAT_TEXT_COLOR,
                    chat_bubble_color: Character::DEFAULT_CHAT_BUBBLE_COLOR,
                    chat_scale: 1,
                    scale: 1.0,
                    pos: self.config.pos,
                    rot: Pos::default(),
                    spawn_animation_id: -1,
                    attachments: Vec::new(),
                    hostility: Hostility::Neutral,
                    unknown10: 0,
                    texture_alias: "".to_string(),
                    tint_name: "".to_string(),
                    tint_id: 0,
                    unknown11: false,
                    offset_y: 0.0,
                    composite_effect: 0,
                    wield_type: self.config.ai.wield_type,
                    name_override: "".to_string(),
                    hide_name: true,
                    name_offset_x: 0.0,
                    name_offset_y: 0.0,
                    name_offset_z: 0.0,
                    terrain_object_id: 0,
                    invisible: false,
                    speed: 0.0,
                    unknown21: false,
                    interactable_size_pct: 0,
                    unknown23: -1,
                    unknown24: -1,
                    looping_animation_id: -1,
                    unknown26: false,
                    disable_gravity: false,
                    sub_title_id: 0,
                    one_shot_animation_id: -1,
                    temporary_model: 0,
                    effects: Vec::new(),
                    disable_interact_popup: true,
                    unknown33: 0,
                    unknown34: false,
                    show_health: false,
                    hide_despawn_fade: true,
                    enable_tilt: false,
                    base_attachment_group: BaseAttachmentGroup {
                        unknown1: 0,
                        unknown2: "".to_string(),
                        unknown3: "".to_string(),
                        unknown4: 0,
                        unknown5: "".to_string(),
                    },
                    tilt: Pos::default(),
                    unknown40: 0,
                    bounce_area_id: -1,
                    image_set_id: 0,
                    collision: false,
                    rider_guid: 0,
                    npc_type: 2,
                    interact_popup_radius: 0.0,
                    target: Target::None,
                    variables: Vec::new(),
                    rail_id: 0,
                    rail_elapsed_seconds: 0.0,
                    rail_offset: Pos::default(),
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
            }));
        }

        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: SaberDuelStageData {
                minigame_header: MinigameHeader {
                    stage_guid: self.stage_guid,
                    sub_op_code: SaberDuelOpCode::StageData as i32,
                    stage_group_guid: self.stage_group_guid,
                },
                win_score: self.config.bouts_to_win_round.into(),
                total_rounds: self.config.rounds_to_win.into(),
                seconds_remaining: 0,
                camera_pos: self.config.pos,
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
                    .map(|_| self.config.player_entrance_animation_id)
                    .unwrap_or(self.config.ai.entrance_animation_id),
                opponent_entrance_sound_id: self
                    .player2
                    .map(|_| 0)
                    .unwrap_or(self.config.ai.entrance_sound_id),
                max_force_points: self.config.max_force_points.into(),
                paused: false,
                enable_memory_challenge: self.config.memory_challenge,
                force_powers: self
                    .config
                    .force_powers
                    .iter()
                    .map(|force_power| force_power.definition.clone())
                    .collect(),
            },
        }));

        Ok(packets)
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

        let completion_time = match player_index == 0 {
            true => player1_completed_time,
            false => player2_completed_time,
        };

        let now = Instant::now();

        if completion_time.is_some() || bout_time_remaining.time_until_next_event(now).is_zero() {
            return Ok(Vec::new());
        }

        let keypress = event.keypress;
        let player_state = &mut self.player_states[player_index as usize];

        let is_reverse = player_state.is_affected_by(SaberDuelForcePower::RightToLeft);
        let key_index = match is_reverse {
            true => player_state
                .required_progress
                .saturating_sub(player_state.progress) as usize,
            false => player_state.progress as usize,
        }
        .min(keys.len() - 1);

        if keys[key_index] == keypress.into() {
            if player_state.increment_progress() {
                *completion_time = Some(now);
            }
        } else {
            player_state.make_mistake();
        }

        Ok(self.update_progress(player_index))
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        match &mut self.state {
            SaberDuelGameState::WaitingForForcePowers { timer } => {
                if timer.time_until_next_event(now).is_zero() {
                    self.start_bout()
                } else {
                    Vec::new()
                }
            }
            SaberDuelGameState::BoutActive {
                bout_time_remaining,
                is_long_bout,
                ai_next_key,
                player1_completed_time,
                player2_completed_time,
                ..
            } => {
                if bout_time_remaining.time_until_next_event(now).is_zero() {
                    return self.tie_bout();
                }

                let is_long_bout = *is_long_bout;

                let bout_completion = match (&player1_completed_time, &player2_completed_time) {
                    (None, None) => SaberDuelBoutCompletion::NeitherPlayer,
                    (None, Some(player2_time)) => SaberDuelBoutCompletion::OnePlayer {
                        time_since_completion: now.saturating_duration_since(*player2_time),
                        player_index: 1,
                    },
                    (Some(player1_time), None) => SaberDuelBoutCompletion::OnePlayer {
                        time_since_completion: now.saturating_duration_since(*player1_time),
                        player_index: 0,
                    },
                    (Some(player1_time), Some(player2_time)) => {
                        let (min, max, fastest_player_index) = match player1_time < player2_time {
                            true => (player1_time, player2_time, 0),
                            false => (player2_time, player1_time, 1),
                        };

                        SaberDuelBoutCompletion::BothPlayers {
                            time_between_completions: max.saturating_duration_since(*min),
                            fastest_player_index,
                        }
                    }
                };

                match bout_completion {
                    SaberDuelBoutCompletion::NeitherPlayer => {}
                    SaberDuelBoutCompletion::OnePlayer {
                        time_since_completion,
                        player_index,
                    } => {
                        if time_since_completion
                            > Duration::from_millis(self.config.tie_interval_millis.into())
                        {
                            return self.win_bout(player_index, is_long_bout);
                        }
                    }
                    SaberDuelBoutCompletion::BothPlayers {
                        time_between_completions,
                        fastest_player_index,
                    } => {
                        if time_between_completions
                            > Duration::from_millis(self.config.tie_interval_millis.into())
                        {
                            return self.win_bout(fastest_player_index, is_long_bout);
                        }

                        return self.tie_bout();
                    }
                }

                if Self::tick_ai(
                    now,
                    &self.config,
                    &mut self.player_states[1],
                    ai_next_key,
                    player2_completed_time,
                ) {
                    self.update_progress(1)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    pub fn remove_player(
        &self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if self.player2.is_none() {
            Ok(vec![Broadcast::Single(
                player,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: RemoveStandard {
                        guid: saber_duel_opponent_guid(self.player1),
                    },
                })],
            )])
        } else {
            Ok(Vec::new())
        }
    }

    fn mark_player_ready(&mut self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
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
            broadcasts.push(Broadcast::Multi(
                self.recipients.clone(),
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: SaberDuelGameStart {
                        minigame_header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: SaberDuelOpCode::GameStart as i32,
                            stage_group_guid: self.stage_group_guid,
                        },
                    },
                })],
            ));
        }

        broadcasts.append(&mut self.prepare_bout());

        Ok(broadcasts)
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
                        stage_guid: self.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutInfo as i32,
                        stage_group_guid: self.stage_group_guid,
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

        let player1_flags = self.player_states[0].force_power_flags(&self.config.force_powers);
        show_force_power_dialog |= player1_flags.can_use_any();

        let player2_flags = self.player_states[1].force_power_flags(&self.config.force_powers);
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
                        stage_guid: self.stage_guid,
                        sub_op_code: SaberDuelOpCode::ShowForcePowerDialog as i32,
                        stage_group_guid: self.stage_group_guid,
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
                            stage_guid: self.stage_guid,
                            sub_op_code: SaberDuelOpCode::ShowForcePowerDialog as i32,
                            stage_group_guid: self.stage_group_guid,
                        },
                        flags: player2_flags,
                    },
                })],
            ));
        }

        self.state = SaberDuelGameState::WaitingForForcePowers {
            timer: MinigameTimer::new_with_event(Duration::from_millis(
                self.config.force_power_selection_max_millis.into(),
            )),
        };

        broadcasts
    }

    fn start_bout(&mut self) -> Vec<Broadcast> {
        self.bout = self.bout.saturating_add(1);
        let is_long_bout = self.bout >= self.config.first_long_bout
            && (self.bout - self.config.first_long_bout) % self.config.long_bout_interval == 0;

        let base_sequence_len = if is_long_bout {
            self.config.keys_per_long_bout
        } else {
            self.config.keys_per_short_bout
        };

        // Add a key for the extra key force power
        let extra_key_sequence_len = base_sequence_len.saturating_add(1);

        let mut keys: Vec<SaberDuelKey> = Vec::new();
        for _ in 0..extra_key_sequence_len {
            keys.push(thread_rng().gen());
        }

        self.state = SaberDuelGameState::BoutActive {
            bout_time_remaining: MinigameTimer::new_with_event(Duration::from_millis(
                self.config.bout_max_millis.into(),
            )),
            is_long_bout,
            keys: keys.clone(),
            ai_next_key: MinigameTimer::new_with_event(Duration::from_millis(
                self.config.ai.millis_per_key.into(),
            )),
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

        vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelBoutStart {
                    minigame_header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutStart as i32,
                        stage_group_guid: self.stage_group_guid,
                    },
                    keys,
                    num_keys_by_player_index: vec![player1_keys.into(), player2_keys.into()],
                },
            })],
        )]
    }

    #[must_use]
    fn tick_ai(
        now: Instant,
        config: &SaberDuelConfig,
        player_state: &mut SaberDuelPlayerState,
        ai_next_key: &mut MinigameTimer,
        bout_completed_time: &mut Option<Instant>,
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
        } else if player_state.increment_progress() {
            *bout_completed_time = Some(now);
        }
        ai_next_key.schedule_event(Duration::from_millis(config.ai.millis_per_key.into()));

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
                        stage_guid: self.stage_guid,
                        sub_op_code: SaberDuelOpCode::PlayerUpdate as i32,
                        stage_group_guid: self.stage_group_guid,
                    },
                    player_index: player_index.into(),
                    progress: player_state.progress.into(),
                },
            })],
        )]
    }

    fn win_bout(&mut self, winner_index: u8, is_long_bout: bool) -> Vec<Broadcast> {
        self.reset_readiness(false);

        let loser_index = (winner_index + 1) % 2;
        let loser_state = &mut self.player_states[loser_index as usize];
        loser_state.add_force_points(
            self.config.force_points_per_bout_lost,
            self.config.max_force_points,
        );

        let winner_state = &mut self.player_states[winner_index as usize];
        winner_state.add_force_points(
            self.config.force_points_per_bout_won,
            self.config.max_force_points,
        );
        let score_per_bout_won = 1;
        winner_state.bouts_won = winner_state.bouts_won.saturating_add(score_per_bout_won);

        let rng = &mut thread_rng();
        let animation_pair = match is_long_bout {
            true => {
                &self.config.long_bout_animations[self.long_bout_animation_distribution.sample(rng)]
            }
            false => {
                &self.config.short_bout_animations
                    [self.short_bout_animation_distribution.sample(rng)]
            }
        };

        let mut broadcasts = vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelBoutWon {
                    minigame_header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutWon as i32,
                        stage_group_guid: self.stage_group_guid,
                    },
                    winner_index: winner_index.into(),
                    added_score: score_per_bout_won.into(),
                    winner_animation_id: animation_pair.attack_animation_id,
                    loser_animation_id: animation_pair.defend_animation_id,
                },
            })],
        )];

        if winner_state.bouts_won >= self.config.bouts_to_win_round {
            winner_state.rounds_won = winner_state.rounds_won.saturating_add(1);
            self.bout = 0;

            // TODO: reset bouts won for both players

            broadcasts.push(Broadcast::Multi(
                self.recipients.clone(),
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: SaberDuelRoundOver {
                        minigame_header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: SaberDuelOpCode::BoutWon as i32,
                            stage_group_guid: self.stage_group_guid,
                        },
                        winner_index: winner_index.into(),
                        sound_id: match self.player2.is_none() {
                            true => match winner_index == 0 {
                                true => self.config.ai.bout_lost_sound_id,
                                false => self.config.ai.bout_won_sound_id,
                            },
                            false => 0,
                        },
                    },
                })],
            ));
        }

        if winner_state.rounds_won == self.config.rounds_to_win {
            // TODO: handle player won game
        }

        broadcasts
    }

    fn tie_bout(&mut self) -> Vec<Broadcast> {
        self.reset_readiness(false);
        self.player_states.iter_mut().for_each(|player_state| {
            player_state.add_force_points(
                self.config.force_points_per_bout_tied,
                self.config.max_force_points,
            )
        });

        vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SaberDuelBoutTied {
                    minigame_header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: SaberDuelOpCode::BoutTied as i32,
                        stage_group_guid: self.stage_group_guid,
                    },
                },
            })],
        )]
    }

    fn reset_readiness(&mut self, game_start: bool) {
        self.state = SaberDuelGameState::WaitingForPlayersReady { game_start };
        self.player_states[0].ready = false;
        self.player_states[1].ready = self.player2.is_none();
    }
}

use std::{
    io::{Cursor, Read},
    time::{Duration, Instant},
};

use enum_iterator::all;
use packet_serialize::DeserializePacket;
use rand::{thread_rng, Rng};
use serde::Deserialize;

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
            SaberDuelBoutInfo, SaberDuelBoutStart, SaberDuelBoutTied, SaberDuelForcePower,
            SaberDuelForcePowerDefinition, SaberDuelForcePowerFlags, SaberDuelGameStart,
            SaberDuelKey, SaberDuelOpCode, SaberDuelPlayerUpdate, SaberDuelShowForcePowerDialog,
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
    mistake_probability: f32,
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

    pub fn increment_progress(&mut self) -> bool {
        let new_progress = self.progress.saturating_add(1).min(self.required_progress);
        self.progress = new_progress;

        new_progress >= self.required_progress
    }
}

#[derive(Clone, Debug, Deserialize)]
struct SaberDuelAnimationPair {
    attack_animation_id: i32,
    defend_animation_id: i32,
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
    WaitingForPlayersReady,
    WaitingForForcePowers {
        timer: MinigameTimer,
    },
    BoutActive {
        bout_time_remaining: MinigameTimer,
        keys: Vec<SaberDuelKey>,
        ai_next_key: MinigameTimer,
        player1_completed_time: Option<Instant>,
        player2_completed_time: Option<Instant>,
    },
    BoutEnded,
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
    short_bout_animations: Vec<SaberDuelAnimationPair>,
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
    right_to_left_ai_mistake_multiplier: f32,
    opposite_ai_mistake_multiplier: f32,
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
                    SaberDuelOpCode::Keypress => Ok(Vec::new()),
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
        let mut player2_state = SaberDuelPlayerState::default();
        if player2.is_none() {
            player2_state.ready = true;
        }

        let mut recipients = vec![player1];
        if let Some(player2) = player2 {
            recipients.push(player2);
        }

        SaberDuelGame {
            config,
            player1,
            player2,
            player_states: [SaberDuelPlayerState::default(), player2_state],
            bout: 0,
            state: SaberDuelGameState::WaitingForPlayersReady,
            recipients,
            stage_guid,
            stage_group_guid,
        }
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
                local_player_index: player_index,
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
                keys,
                ai_next_key,
                player1_completed_time,
                player2_completed_time,
            } => {
                if bout_time_remaining.time_until_next_event(now).is_zero() {
                    return self.tie();
                }

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
                            // handle player win
                        }
                    }
                    SaberDuelBoutCompletion::BothPlayers {
                        time_between_completions,
                        fastest_player_index,
                    } => {
                        if time_between_completions
                            > Duration::from_millis(self.config.tie_interval_millis.into())
                        {
                            // handle player win
                        }

                        return self.tie();
                    }
                }

                if self.player2.is_none() && ai_next_key.time_until_next_event(now).is_zero() {
                    // TODO: handle mistakes
                    if self.player_states[1].increment_progress() {
                        *player2_completed_time = Some(now);
                    }
                    ai_next_key.schedule_event(Duration::from_millis(
                        self.config.ai.millis_per_key.into(),
                    ));

                    return self.update_progress(1);
                }

                Vec::new()
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

        if self.player_states[player_index].ready {
            return Ok(Vec::new());
        }
        self.player_states[player_index].ready = true;

        if !self.player_states[0].ready || !self.player_states[1].ready {
            return Ok(Vec::new());
        }

        let mut broadcasts = vec![Broadcast::Multi(
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
        )];

        broadcasts.append(&mut self.prepare_bout());

        Ok(broadcasts)
    }

    fn player_index(&self, sender: u32) -> Result<u32, ProcessPacketError> {
        if sender == self.player1 {
            Ok(0)
        } else if Some(sender) == self.player2 {
            Ok(1)
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {sender} sent a ready packet for Saber Duel, but they aren't one of the game's players ({self:?})")
            ))
        }
    }

    fn start_round(&mut self) -> Vec<Broadcast> {
        self.bout = 0;
        self.prepare_bout()
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

        self.player_states[0].required_progress = player1_keys;
        self.player_states[1].required_progress = player2_keys;

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

    fn tie(&mut self) -> Vec<Broadcast> {
        self.state = SaberDuelGameState::BoutEnded;
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
}

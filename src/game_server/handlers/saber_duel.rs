use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    packets::{
        item::WieldType,
        minigame::MinigameHeader,
        saber_duel::{SaberDuelForcePower, SaberDuelOpCode},
    },
    Broadcast, ProcessPacketError, ProcessPacketErrorType,
};

#[derive(Debug)]
struct SaberDuelAiForcePower {
    force_power: SaberDuelForcePower,
    weight: u8,
}

#[derive(Debug)]
struct SaberDuelAi {
    name_id: u32,
    model_id: u32,
    wield_type: WieldType,
    entrance_animation_id: u32,
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

#[derive(Debug)]
struct SaberDuelAppliedForcePower {
    force_power: SaberDuelForcePower,
    bouts_remaining: u8,
}

#[derive(Debug, Default)]
struct SaberDuelPlayerState {
    ready: bool,
    rounds_won: u8,
    bouts_won: u8,
    progress: u8,
    affected_by_force_powers: Vec<SaberDuelAppliedForcePower>,
    saw_force_power_tutorial: bool,
}

impl SaberDuelPlayerState {
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn mark_ready(&mut self) {
        self.ready = true;
    }
}

#[derive(Debug)]
struct SaberDuelAnimationPair {
    attack_animation_id: u32,
    defend_animation_id: u32,
    weight: u8,
}

#[derive(Debug)]
pub struct SaberDuelConfig {
    rounds_to_win: u8,
    bouts_to_win_round: u8,
    keys_per_short_bout: u8,
    keys_per_long_bout: u8,
    first_long_bout: u8,
    long_bout_interval: u8,
    bout_max_millis: u32,
    short_bout_animations: Vec<SaberDuelAnimationPair>,
    long_bout_animations: Vec<SaberDuelAnimationPair>,
    establishing_animation_id: u32,
    player_entrance_animation_id: u32,
    ai: SaberDuelAi,
    max_force_points: u8,
    force_points_per_bout_won: u8,
    force_points_per_bout_tied: u8,
    force_points_per_bout_lost: u8,
    force_power_tutorial: Option<SaberDuelForcePower>,
    right_to_left_ai_mistake_multiplier: f32,
    opposite_ai_mistake_multiplier: f32,
    memory_challenge: bool,
}

#[derive(Debug)]
pub struct SaberDuelGame {
    config: SaberDuelConfig,
    player1: u32,
    player2: Option<u32>,
    player_states: [SaberDuelPlayerState; 2],
    bout: u8,
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
        SaberDuelGame {
            config,
            player1,
            player2,
            player_states: [
                SaberDuelPlayerState::default(),
                SaberDuelPlayerState::default(),
            ],
            bout: 0,
            stage_guid,
            stage_group_guid,
        }
    }

    pub fn process_packet(
        &mut self,
        cursor: &mut Cursor<&[u8]>,
        sender: u32,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let header = MinigameHeader::deserialize(cursor)?;
        match SaberDuelOpCode::try_from(header.sub_op_code) {
            Ok(op_code) => match op_code {
                SaberDuelOpCode::PlayerReady => self.mark_player_ready(sender),
                SaberDuelOpCode::Keypress => todo!(),
                SaberDuelOpCode::RequestApplyForcePower => todo!(),
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
    }

    fn mark_player_ready(&mut self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if sender == self.player1 {
            if self.player_states[0].is_ready() {
                return Ok(Vec::new());
            }
            self.player_states[0].mark_ready();
        } else if Some(sender) == self.player2 {
            if self.player_states[1].is_ready() {
                return Ok(Vec::new());
            }
            self.player_states[1].mark_ready();
        } else {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} sent a ready packet for Saber Duel, but they aren't one of the game's players ({self:?})")));
        }

        if !self.player_states[0].is_ready() || !self.player_states[1].is_ready() {
            return Ok(Vec::new());
        }

        Ok(Vec::new())
    }
}

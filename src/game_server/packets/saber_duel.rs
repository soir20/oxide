use num_enum::{IntoPrimitive, TryFromPrimitive};
use packet_serialize::{DeserializePacket, SerializePacket};
use serde::Deserialize;

use super::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket, Pos,
};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(i32)]
pub enum SaberDuelOpCode {
    StageData = 0x1,
    OpponentGuid = 0x2,
    GameStart = 0x3,
    BoutInfo = 0x4,
    ShowForcePowerDialog = 0x5,
    ApplyForcePower = 0x6,
    RemoveForcePower = 0x7,
    BoutStart = 0x8,
    PlayerUpdate = 0x9,
    BoutWon = 0xa,
    BoutTied = 0xb,
    RoundOver = 0xc,
    GameOver = 0xd,
    TriggerBoost = 0xe,
    SetMemoryChallenge = 0xf,
    PlayerReady = 0x10,
    Keypress = 0x11,
    RequestApplyForcePower = 0x12,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    SerializePacket,
    DeserializePacket,
)]
#[repr(u32)]
pub enum SaberDuelForcePower {
    ExtraKey = 0,
    RightToLeft = 1,
    Opposite = 2,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelForcePowerDefinition {
    pub force_power: SaberDuelForcePower,
    pub name_id: u32,
    pub small_icon_set_id: u32,
    pub icon_set_id: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelStageData {
    pub minigame_header: MinigameHeader,
    pub win_score: u32,
    pub total_rounds: u32,
    pub seconds_remaining: u32,
    pub camera_position: Pos,
    pub camera_rotation: f32,
    pub max_combo_points: u32,
    pub establishing_animation_id: i32,
    pub local_player_index: u32,
    pub opponent_guid: u64,
    pub opponent_entrance_animation_id: i32,
    pub opponent_entrance_sound_id: u32,
    pub max_force_points: u32,
    pub paused: bool,
    pub enable_memory_challenge: bool,
    pub force_powers: Vec<SaberDuelForcePowerDefinition>,
}

impl GamePacket for SaberDuelStageData {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelOpponentGuid {
    pub minigame_header: MinigameHeader,
    pub guid: u64,
}

impl GamePacket for SaberDuelOpponentGuid {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelGameStart {
    pub minigame_header: MinigameHeader,
}

impl GamePacket for SaberDuelGameStart {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelBoutInfo {
    pub minigame_header: MinigameHeader,
    pub max_bout_time_millis: u32,
    pub is_combo_bout: bool,
    pub force_points_by_player_index: Vec<u32>,
}

impl GamePacket for SaberDuelBoutInfo {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

pub struct SaberDuelForcePowerFlags {
    pub can_use_extra_key: bool,
    pub can_use_right_to_left: bool,
    pub can_use_opposite: bool,
}

impl SerializePacket for SaberDuelForcePowerFlags {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let mut value: u8 = 0;
        if self.can_use_extra_key {
            value |= 0x80;
        }
        if self.can_use_right_to_left {
            value |= 0x40;
        }
        if self.can_use_opposite {
            value |= 0x20;
        }

        value.serialize(buffer);
    }
}

#[derive(SerializePacket)]
pub struct SaberDuelShowForcePowerDialog {
    pub minigame_header: MinigameHeader,
    pub flags: SaberDuelForcePowerFlags,
}

impl GamePacket for SaberDuelShowForcePowerDialog {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket)]
pub struct SaberDuelApplyForcePower {
    pub minigame_header: MinigameHeader,
    pub used_by_player_index: u32,
    pub force_power: SaberDuelForcePower,
    pub bouts_remaining: u32,
    pub new_force_points: u32,
    pub animation_id: u32,
    pub flags: SaberDuelForcePowerFlags,
}

impl GamePacket for SaberDuelApplyForcePower {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelRemoveForcePower {
    pub minigame_header: MinigameHeader,
    pub used_by_player_index: u32,
    pub force_power: SaberDuelForcePower,
}

impl GamePacket for SaberDuelRemoveForcePower {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(Clone, Copy, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket)]
#[repr(u32)]
pub enum SaberDuelKey {
    Up = 1,
    Down = 2,
    Left = 3,
    Right = 4,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelBoutStart {
    pub minigame_header: MinigameHeader,
    pub keys: Vec<SaberDuelKey>,
    pub num_keys_by_player_index: Vec<u32>,
}

impl GamePacket for SaberDuelBoutStart {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelPlayerUpdate {
    pub minigame_header: MinigameHeader,
    pub player_index: u32,
    pub current_key_index: u32,
}

impl GamePacket for SaberDuelPlayerUpdate {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelBoutWon {
    pub minigame_header: MinigameHeader,
    pub winner_index: u32,
    pub new_score: u32,
    pub winner_animation_id: u32,
    pub loser_animation_id: u32,
}

impl GamePacket for SaberDuelBoutWon {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelBoutTied {
    pub minigame_header: MinigameHeader,
}

impl GamePacket for SaberDuelBoutTied {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelRoundOver {
    pub minigame_header: MinigameHeader,
    pub winner_index: u32,
    pub sound_id: u32,
}

impl GamePacket for SaberDuelRoundOver {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelGameOver {
    pub minigame_header: MinigameHeader,
    pub winner_index: u32,
    pub sound_id: u32,
    pub round_won: bool,
    pub challenge_failed: bool,
}

impl GamePacket for SaberDuelGameOver {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(Clone, Copy, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket)]
#[repr(u32)]
pub enum SaberDuelBoost {
    TwoKeys = 0,
    ForgiveMistake = 1,
    DoubleScore = 2,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelTriggerBoost {
    pub minigame_header: MinigameHeader,
    pub player_index: u32,
    pub boost: SaberDuelBoost,
}

impl GamePacket for SaberDuelTriggerBoost {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelSetMemoryChallenge {
    pub minigame_header: MinigameHeader,
    pub enabled: bool,
}

impl GamePacket for SaberDuelSetMemoryChallenge {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelKeypress {
    pub minigame_header: MinigameHeader,
    pub keypress: SaberDuelKey,
}

impl GamePacket for SaberDuelKeypress {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelRequestApplyForcePower {
    pub minigame_header: MinigameHeader,
    pub force_power: SaberDuelForcePower,
}

impl GamePacket for SaberDuelRequestApplyForcePower {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket};

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
    RoundInfo = 0x4,
    RoundStart = 0x8,
    PlayerUpdate = 0x9,
    RoundWon = 0xa,
    SetOver = 0xc,
    GameOver = 0xd,
    PlayerReady = 0x10,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelForcePowerDefinition {
    pub guid: u32,
    pub name_id: u32,
    pub small_icon_set_id: u32,
    pub icon_set_id: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelStageData {
    pub minigame_header: MinigameHeader,
    pub sets_to_win_round: u32,
    pub total_rounds: u32,
    pub seconds_remaining: u32,
    pub camera_position: Pos,
    pub camera_rotation: f32,
    pub unknown6: u32,
    pub establishing_animation_id: i32,
    pub local_player_index: u32, // Causes crashes
    pub opponent_guid: u64,
    pub opponent_entrance_animation_id: i32,
    pub opponent_entrance_sound_id: u32,
    pub max_force_points: u32,
    pub unknown13: bool,
    pub enable_memory_challenge: bool,
    pub force_powers: Vec<SaberDuelForcePowerDefinition>,
}

impl GamePacket for SaberDuelStageData {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct OpponentGuid {
    pub minigame_header: MinigameHeader,
    pub guid: u64,
}

impl GamePacket for OpponentGuid {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct GameStart {
    pub minigame_header: MinigameHeader,
}

impl GamePacket for GameStart {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RoundInfo {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: bool,
    pub force_points_by_player_index: Vec<u32>,
}

impl GamePacket for RoundInfo {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RoundStart {
    pub minigame_header: MinigameHeader,
    pub unknown1: Vec<SaberDuelForcePowerDefinition>,
    pub unknown2: Vec<SaberDuelForcePowerDefinition>,
}

impl GamePacket for RoundStart {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PlayerUpdate {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
}

impl GamePacket for PlayerUpdate {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RoundWon {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

impl GamePacket for RoundWon {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SetOver {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
}

impl GamePacket for SetOver {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct GameOver {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: bool,
    pub unknown4: bool,
}

impl GamePacket for GameOver {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

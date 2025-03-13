use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket};

use super::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket,
};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(i32)]
pub enum SaberStrikeOpCode {
    StageData = 0x1,
    GameOver = 0x2,
    ObfuscatedScore = 0x9,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberStrikeStageData {
    pub minigame_header: MinigameHeader,
    pub saber_strike_stage_id: u32,
    pub use_player_weapon: bool,
}

impl GamePacket for SaberStrikeStageData {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberStrike;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberStrikeGameOver {
    pub won: bool,
    pub score: u32,
    pub best_throw: u32,
    pub enemies_killed: u32,
    pub duration_seconds: f32,
    pub remaining_sabers: u32,
}

impl GamePacket for SaberStrikeGameOver {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberStrike;
}

fn ror(x: u32, bits: u8) -> u32 {
    let right_shift = (bits as u32) % u32::BITS;
    let left_shift = 32 - right_shift;
    (x >> right_shift) | (x << left_shift)
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberStrikeObfuscatedScore {
    pub minigame_header: MinigameHeader,
    seed: u32,
    obfuscated_score: u32,
}

impl GamePacket for SaberStrikeObfuscatedScore {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberStrike;
}

impl SaberStrikeObfuscatedScore {
    pub fn score(&self) -> u32 {
        self.obfuscated_score ^ ror(self.seed, 1)
    }
}

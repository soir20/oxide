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
    SingleKill = 0x3,
    ThrowKill = 0x4,
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

#[derive(DeserializePacket)]
pub struct SaberStrikeGameOver {
    pub won: bool,
    pub score: u32,
    pub best_throw: u32,
    pub enemies_killed: u32,
    pub duration_seconds: f32,
    pub remaining_sabers: u32,
}

#[derive(DeserializePacket)]
pub struct SaberStrikeSingleKill {
    pub enemy_type: u32,
}

#[derive(DeserializePacket)]
pub struct SaberStrikeThrowKill {
    pub enemies_killed: u32,
}

#[derive(DeserializePacket)]
pub struct SaberStrikeObfuscatedScore {
    seed: u32,
    obfuscated_score: u32,
}

impl SaberStrikeObfuscatedScore {
    pub fn score(&self) -> u32 {
        self.obfuscated_score ^ self.seed.rotate_right(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_zero() {
        let obfuscated_score = SaberStrikeObfuscatedScore {
            seed: 0x113bf61a,
            obfuscated_score: 0x89dfb0d,
        };
        assert_eq!(obfuscated_score.score(), 0)
    }

    #[test]
    fn test_small_score() {
        let obfuscated_score = SaberStrikeObfuscatedScore {
            seed: 0x113bf61a,
            obfuscated_score: 0x89de5f7,
        };
        assert_eq!(obfuscated_score.score(), 7930)
    }

    #[test]
    fn test_large_score() {
        let obfuscated_score = SaberStrikeObfuscatedScore {
            seed: 0x113bf61a,
            obfuscated_score: 0xf76204f2,
        };
        assert_eq!(obfuscated_score.score(), u32::MAX)
    }
}

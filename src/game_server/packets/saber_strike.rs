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
    ExtraSabersBoost = 0x5,
    DemagnetizeWallsBoost = 0x6,
    ReducedGoalBoost = 0x7,
    EnableTrajectoryBoost = 0x8,
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
    pub total_score: i32,
    pub best_throw: i32,
    pub enemies_killed: i32,
    pub duration_seconds: f32,
    pub remaining_sabers: i32,
}

#[allow(dead_code)]
#[derive(DeserializePacket)]
pub struct SaberStrikeSingleKill {
    pub enemy_type: u32,
}

#[allow(dead_code)]
#[derive(DeserializePacket)]
pub struct SaberStrikeThrowKill {
    pub enemies_killed: u32,
}

#[derive(DeserializePacket)]
pub struct ExtraSabersBoost {
    pub minigame_header: MinigameHeader,
    pub sabers: u32,
}

#[derive(DeserializePacket)]
pub struct DemagnetizeWallsBoost {
    pub minigame_header: MinigameHeader,
}

#[derive(DeserializePacket)]
pub struct ReducedGoalBoost {
    pub minigame_header: MinigameHeader,
    pub goal_deduction: u32,
}

#[derive(DeserializePacket)]
pub struct EnableTrajectoryBoost {
    pub minigame_header: MinigameHeader,
}

#[derive(DeserializePacket)]
pub struct SaberStrikeObfuscatedScore {
    seed: i32,
    obfuscated_score: i32,
}

impl SaberStrikeObfuscatedScore {
    pub fn score(&self) -> i32 {
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
            obfuscated_score: 0x776204f2,
        };
        assert_eq!(obfuscated_score.score(), i32::MAX)
    }
}

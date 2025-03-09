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

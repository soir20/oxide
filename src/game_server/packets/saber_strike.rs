use packet_serialize::{DeserializePacket, SerializePacket};

use super::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket,
};

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberStrikeInit {
    pub minigame_header: MinigameHeader,
    pub stage_id: u32,
    pub disable_default_saber: bool,
}

impl GamePacket for SaberStrikeInit {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberStrike;
}

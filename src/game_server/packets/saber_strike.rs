use packet_serialize::{DeserializePacket, SerializePacket};

use super::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket,
};

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberStrikeInit {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: bool,
}

impl GamePacket for SaberStrikeInit {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberStrike;
}

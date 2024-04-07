use packet_serialize::{DeserializePacket, SerializePacket};
use crate::game_server::game_packet::{GamePacket, OpCode, Pos};

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdatePlayerPosition {
    pub guid: u64,
    pub pos: Pos,
    pub rot: Pos,
    pub character_state: u8,
    pub unknown: u8
}

impl GamePacket for UpdatePlayerPosition {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::UpdatePlayerPosition;
}

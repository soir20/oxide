use packet_serialize::{DeserializePacket, SerializePacket};

use crate::game_server::game_packet::{GamePacket, OpCode};

#[derive(Clone, SerializePacket, DeserializePacket)]
pub struct UpdatePlayerPosition {
    pub guid: u64,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub character_state: u8,
    pub unknown: u8,
}

impl GamePacket for UpdatePlayerPosition {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::UpdatePlayerPosition;
}

use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
pub struct UpdatePlayerPosition {
    pub guid: u64,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub stop_at_destination: bool,
    pub unknown: u8,
}

impl GamePacket for UpdatePlayerPosition {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::UpdatePlayerPosition;
}

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
pub struct PlayerJump {
    pub pos_update: UpdatePlayerPosition,
    pub vertical_speed: f32,
}

impl GamePacket for PlayerJump {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::PlayerJump;
}

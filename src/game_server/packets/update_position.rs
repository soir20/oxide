use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode, Pos};

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
pub struct UpdatePlayerPos {
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

impl GamePacket for UpdatePlayerPos {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::UpdatePlayerPos;
}

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
pub struct PlayerJump {
    pub pos_update: UpdatePlayerPos,
    pub vertical_speed: f32,
}

impl GamePacket for PlayerJump {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::PlayerJump;
}

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
pub struct UpdatePlayerPlatformPos {
    pub pos_update: UpdatePlayerPos,
    pub platform_guid: u64,
    pub player_pos_relative_to_platform: Pos,
}

impl GamePacket for UpdatePlayerPlatformPos {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::UpdatePlayerPlatformPos;
}

use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode, Pos};

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
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

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
pub struct PlayerJump {
    pub pos_update: UpdatePlayerPosition,
    pub vertical_speed: f32,
}

impl GamePacket for PlayerJump {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::PlayerJump;
}

#[derive(Copy, Clone, SerializePacket, DeserializePacket)]
pub struct UpdatePlayerPlatformPosition {
    pub pos_update: UpdatePlayerPosition,
    pub platform_guid: u64,
    pub player_pos_relative_to_platform: Pos,
}

impl GamePacket for UpdatePlayerPlatformPosition {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::UpdatePlayerPlatformPosition;
}

use num_enum::{IntoPrimitive, TryFromPrimitive};
use packet_serialize::{DeserializePacket, SerializePacket};

use crate::game_server::packets::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket,
};

#[derive(
    Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket,
)]
#[repr(i32)]
pub enum AttackCruiserInitOpCode {
    ClientConfig = 0x1,
}

#[derive(SerializePacket)]
pub struct AttackCruiserConfig {
    pub unknown1: i32,
    pub unknown2: i32,
    pub unknown3: String,
    pub config_type: AttackCruiserConfigType,
}

pub enum AttackCruiserConfigType {
    Global {},
}

impl SerializePacket for AttackCruiserConfigType {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        match self {
            AttackCruiserConfigType::Global { .. } => (0..260).for_each(|_| 0u8.serialize(buffer)),
        }
    }
}

pub struct AttackCruiserGameConfig {
    pub minigame_header: MinigameHeader,
    pub config1: AttackCruiserConfig,
    pub config2: AttackCruiserConfig,
    pub config3: AttackCruiserConfig,
    pub configs: Vec<AttackCruiserConfig>,
}

impl SerializePacket for AttackCruiserGameConfig {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.minigame_header.serialize(buffer);
        (self.configs.len() as u32).serialize(buffer);
        self.config1.serialize(buffer);
        self.config2.serialize(buffer);
        self.config3.serialize(buffer);
        self.configs
            .iter()
            .for_each(|config| config.serialize(buffer));
    }
}

impl GamePacket for AttackCruiserGameConfig {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(
    Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket,
)]
#[repr(i32)]
pub enum AttackCruiserClientOpCode {
    UpdatePlayerStates = 0x6,
    UpdateAcotrs = 0x8,
    ClickOnLocation = 0xa,
    RoundTrip = 0x13,
}

#[derive(
    Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket,
)]
#[repr(i32)]
pub enum AttackCruiserServerOpCode {
    UpdateGameState = 0x0,
    AddPlayer = 0x1,
    RemovePlayer = 0x2,
    ConfigPlayer = 0x3,
    UpdatePlayers = 0x5,
    UpdateActors = 0x6,
    AddProjectile = 0x9,
    RemoveProjectile = 0xa,
    AddActor = 0xb,
    RemoveActor = 0xc,
    WorldEffect = 0xd,
    AddScore = 0xe,
    DebugRender = 0xf,
    DebugDrawData = 0x10,
    RoundTrip = 0x11,
    QueueCommand = 0x12,
    UpdateBossCount = 0x13,
}

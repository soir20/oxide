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
pub enum AttackCruiserOpCode {
    ClientConfig = 0x1,
    UpdateGameState = 0x2,
    AddPlayer = 0x3,
    RemovePlayer = 0x4,
    ConfigPlayer = 0x5,
    UpdatePlayerStates = 0x6,
    UpdatePlayers = 0x7,
    UpdateActors = 0x8,
    ClickOnLocation = 0xa,
    AddProjectile = 0xb,
    RemoveProjectile = 0xc,
    AddActor = 0xd,
    RemoveActor = 0xe,
    WorldEffect = 0xf,
    AddScore = 0x10,
    DebugRender = 0x11,
    DebugDrawData = 0x12,
    RoundTrip = 0x13,
    QueueCommand = 0x14,
    UpdateBossCount = 0x15,
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

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserRoundTrip {
    pub minigame_header: MinigameHeader,
    pub client_timestamp: u64,
    pub server_timestamp: u64,
}

impl GamePacket for AttackCruiserRoundTrip {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

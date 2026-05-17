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
pub struct AttackCruiserUpdateGameState {
    pub minigame_header: MinigameHeader,
    pub game_state: u32,
}

impl GamePacket for AttackCruiserUpdateGameState {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

struct AttackCruiserPlayerUpdateType {
    pub unknown1: bool,
    pub unknown2: bool,
    pub unknown3: bool,
    pub unknown4: bool,
    pub unknown5: bool,
}

impl SerializePacket for AttackCruiserPlayerUpdateType {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let mut value = 0;
        if self.unknown1 {
            value |= 0b1;
        }
        if self.unknown2 {
            value |= 0b10;
        }
        if self.unknown3 {
            value |= 0b100;
        }
        if self.unknown4 {
            value |= 0b1000;
        }
        if self.unknown5 {
            value |= 0b10000;
        }

        value.serialize(buffer);
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown1 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: String,
    pub unknown5: String,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown2 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown3 {
    pub unknown1: u32,
    pub unknown2: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown4 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown5 {
    pub unknown1: u32,
}

pub struct AttackCruiserPlayerUpdate {
    pub unknown1: Option<AttackCruiserPlayerUpdateUnknown1>,
    pub unknown2: Option<AttackCruiserPlayerUpdateUnknown2>,
    pub unknown3: Option<AttackCruiserPlayerUpdateUnknown3>,
    pub unknown4: Option<AttackCruiserPlayerUpdateUnknown4>,
    pub unknown5: Option<AttackCruiserPlayerUpdateUnknown5>,
}

impl SerializePacket for AttackCruiserPlayerUpdate {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let update_type = AttackCruiserPlayerUpdateType {
            unknown1: self.unknown1.is_some(),
            unknown2: self.unknown2.is_some(),
            unknown3: self.unknown3.is_some(),
            unknown4: self.unknown4.is_some(),
            unknown5: self.unknown5.is_some(),
        };
        update_type.serialize(buffer);

        if let Some(unknown1) = &self.unknown1 {
            unknown1.serialize(buffer);
        }

        if let Some(unknown2) = &self.unknown2 {
            unknown2.serialize(buffer);
        }

        if let Some(unknown3) = &self.unknown3 {
            unknown3.serialize(buffer);
        }

        if let Some(unknown4) = &self.unknown4 {
            unknown4.serialize(buffer);
        }

        if let Some(unknown5) = &self.unknown5 {
            unknown5.serialize(buffer);
        }
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserAddPlayer {
    pub minigame_header: MinigameHeader,
    pub guid: u64,
    pub update: AttackCruiserPlayerUpdate,
}

impl GamePacket for AttackCruiserAddPlayer {
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

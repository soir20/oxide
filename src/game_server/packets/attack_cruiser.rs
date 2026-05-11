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

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserConfig {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: String,
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

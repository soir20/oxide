use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, OpCode};

pub enum ClientUpdateOpCode {
    Health                   = 0x1,
    Power                    = 0xd,
    Stats                    = 0x7,
    PreloadCharactersDone    = 0x1a
}

pub trait ClientUpdatePacket: SerializePacket {
    const OP_CODE: ClientUpdateOpCode;
}

impl<T: ClientUpdatePacket> GamePacket for T {
    const OP_CODE: OpCode = OpCode::ClientUpdate;

    fn serialize(&self) -> Result<Vec<u8>, SerializePacketError> {
        let mut buffer = GamePacket::serialize_header(self)?;
        buffer.write_u16::<LittleEndian>(Self::OP_CODE as u16)?;
        SerializePacket::serialize(self, &mut buffer)?;
        Ok(buffer)
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Health {
    pub(crate) unknown1: u32,
    pub(crate) unknown2: u32,
}

impl ClientUpdatePacket for Health {
    const OP_CODE: ClientUpdateOpCode = ClientUpdateOpCode::Health;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Power {
    pub(crate) unknown1: u32,
    pub(crate) unknown2: u32,
}

impl ClientUpdatePacket for Power {
    const OP_CODE: ClientUpdateOpCode = ClientUpdateOpCode::Power;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Stat {
    pub(crate) id1: u32,
    pub(crate) id2: u32,
    pub(crate) value1: f32,
    pub(crate) value2: f32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Stats {
    pub(crate) stats: Vec<Stat>
}

impl ClientUpdatePacket for Stats {
    const OP_CODE: ClientUpdateOpCode = ClientUpdateOpCode::Stats;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PreloadCharactersDone {
    pub(crate) unknown1: bool
}

impl ClientUpdatePacket for PreloadCharactersDone {
    const OP_CODE: ClientUpdateOpCode = ClientUpdateOpCode::PreloadCharactersDone;
}

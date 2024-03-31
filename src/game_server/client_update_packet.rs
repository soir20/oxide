use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, OpCode, Pos};

#[derive(Copy, Clone, Debug)]
pub enum ClientUpdateOpCode {
    Health                   = 0x1,
    Position                 = 0xc,
    Power                    = 0xd,
    Stats                    = 0x7,
    PreloadCharactersDone    = 0x1a
}

impl SerializePacket for ClientUpdateOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::ClientUpdate.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Position {
    pub player_pos: Pos,
    pub rot: Pos,
    pub is_teleport: bool,
    pub unknown2: bool
}

impl GamePacket for Position {
    type Header = ClientUpdateOpCode;
    const HEADER: Self::Header = ClientUpdateOpCode::Position;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Health {
    pub(crate) unknown1: u32,
    pub(crate) unknown2: u32,
}

impl GamePacket for Health {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::Health;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Power {
    pub(crate) unknown1: u32,
    pub(crate) unknown2: u32,
}

impl GamePacket for Power {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::Power;
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

impl GamePacket for Stats {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::Stats;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PreloadCharactersDone {
    pub(crate) unknown1: bool
}

impl GamePacket for PreloadCharactersDone {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::PreloadCharactersDone;
}

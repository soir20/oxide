use crate::game_server::game_packet::{GamePacket, OpCode};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use packet_serialize::{
    DeserializePacket, DeserializePacketError, SerializePacket, SerializePacketError,
};
use std::io::{Cursor, Read, Write};

fn serialize_tunneled_packet_from_game_packet<T: GamePacket>(
    buffer: &mut Vec<u8>,
    unknown1: bool,
    inner: &T,
) -> Result<(), SerializePacketError> {
    buffer.write_u8(unknown1 as u8)?;

    let inner_buffer = GamePacket::serialize(inner)?;
    buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
    buffer.write_all(&inner_buffer)?;
    Ok(())
}

fn serialize_tunneled_packet_from_bytes(
    buffer: &mut Vec<u8>,
    unknown1: bool,
    inner: &[u8],
) -> Result<(), SerializePacketError> {
    buffer.write_u8(unknown1 as u8)?;

    buffer.write_u32::<LittleEndian>(inner.len() as u32)?;
    buffer.write_all(inner)?;
    Ok(())
}

fn deserialize_tunneled_packet(
    cursor: &mut Cursor<&[u8]>,
) -> Result<(bool, Vec<u8>), DeserializePacketError> {
    let unknown1 = cursor.read_u8()? != 0;

    let inner_size = cursor.read_u32::<LittleEndian>()?;
    let mut inner = vec![0; inner_size as usize];
    cursor.read_exact(&mut inner)?;

    Ok((unknown1, inner))
}

pub struct TunneledPacket<T> {
    pub unknown1: bool,
    pub inner: T,
}

impl<T: GamePacket> GamePacket for TunneledPacket<T> {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::TunneledClient;
}

impl GamePacket for TunneledPacket<Vec<u8>> {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::TunneledClient;
}

impl<T: GamePacket> SerializePacket for TunneledPacket<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        serialize_tunneled_packet_from_game_packet(buffer, self.unknown1, &self.inner)
    }
}

impl SerializePacket for TunneledPacket<Vec<u8>> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        serialize_tunneled_packet_from_bytes(buffer, self.unknown1, &self.inner)
    }
}

impl DeserializePacket for TunneledPacket<Vec<u8>> {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError>
    where
        Self: Sized,
    {
        let (unknown1, inner) = deserialize_tunneled_packet(cursor)?;
        Ok(TunneledPacket { unknown1, inner })
    }
}

pub struct TunneledWorldPacket<T> {
    pub unknown1: bool,
    pub inner: T,
}

impl<T: GamePacket> GamePacket for TunneledWorldPacket<T> {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::TunneledWorld;
}

impl GamePacket for TunneledWorldPacket<Vec<u8>> {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::TunneledWorld;
}

impl<T: GamePacket> SerializePacket for TunneledWorldPacket<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        serialize_tunneled_packet_from_game_packet(buffer, self.unknown1, &self.inner)
    }
}

impl SerializePacket for TunneledWorldPacket<Vec<u8>> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        serialize_tunneled_packet_from_bytes(buffer, self.unknown1, &self.inner)
    }
}

impl DeserializePacket for TunneledWorldPacket<Vec<u8>> {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError>
    where
        Self: Sized,
    {
        let (unknown1, inner) = deserialize_tunneled_packet(cursor)?;
        Ok(TunneledWorldPacket { unknown1, inner })
    }
}

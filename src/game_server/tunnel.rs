use std::io::{Cursor, Read, Write};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use packet_serialize::{DeserializePacket, DeserializePacketError, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, OpCode};

pub struct TunneledPacket<T> {
    pub unknown1: bool,
    pub inner: T
}

impl<T: GamePacket> GamePacket for TunneledPacket<T> {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::TunneledClient;
}

impl<T: GamePacket> SerializePacket for TunneledPacket<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u8(self.unknown1 as u8)?;

        let inner_buffer = GamePacket::serialize(&self.inner)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;
        Ok(())
    }
}

impl DeserializePacket for TunneledPacket<Vec<u8>> {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError> where Self: Sized {
        let unknown1 = cursor.read_u8()? != 0;

        let inner_size = cursor.read_u32::<LittleEndian>()?;
        let mut inner = vec![0; inner_size as usize];
        cursor.read_exact(&mut inner)?;

        Ok(TunneledPacket {
            unknown1,
            inner,
        })
    }
}

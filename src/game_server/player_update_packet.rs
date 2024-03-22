use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum PlayerUpdateOpCode {
    AddNpc                   = 0x2
}

impl SerializePacket for PlayerUpdateOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::PlayerUpdate.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AddNpc {

}

impl GamePacket for AddNpc {
    type Header = PlayerUpdateOpCode;
    const HEADER: PlayerUpdateOpCode = PlayerUpdateOpCode::AddNpc;
}

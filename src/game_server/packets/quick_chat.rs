use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum QuickChatOpCode {
    QuickChatDefinition = 0x1,
}

impl SerializePacket for QuickChatOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::QuickChat.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Data {
    pub id1: i32,
    pub id2: i32,
    pub menu_text: i32,
    pub chat_text: i32,
    pub animation_id: i32,
    pub unknown1: i32,
    pub admin_only: i32,
    pub menu_icon_id: i32,
    pub item_id: i32,
    pub parent_id: i32,
    pub unknown2: i32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct QuickChatDefinition {
    pub data: Vec<Data>,
}

impl GamePacket for QuickChatDefinition {
    type Header = QuickChatOpCode;
    const HEADER: Self::Header = QuickChatOpCode::QuickChatDefinition;
}

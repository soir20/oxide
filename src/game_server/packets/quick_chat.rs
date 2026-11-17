use packet_serialize::SerializePacket;

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum QuickChatOpCode {
    Definitions = 0x1,
}

impl SerializePacket for QuickChatOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::QuickChat.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(Clone, SerializePacket)]
pub struct QuickChatDefinition {
    pub id: i32,
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

#[derive(SerializePacket)]
pub struct QuickChatDefinitions {
    pub definitions: Vec<QuickChatDefinition>,
}

impl GamePacket for QuickChatDefinitions {
    type Header = QuickChatOpCode;
    const HEADER: Self::Header = QuickChatOpCode::Definitions;
}

use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;

use packet_serialize::{
    DeserializePacket, DeserializePacketError, SerializePacket, SerializePacketError,
};

use super::{GamePacket, OpCode, Pos};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum ChatOpCode {
    SendMessage = 0x1,
}

impl SerializePacket for ChatOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Chat.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum MessageType {
    World = 0x0,
    Whisper = 0x1,
    System = 0x2,
    ReceivedItems = 0x3,
    Group = 0x4,
    Yell = 0x5,
    Trade = 0x6,
    LookingForGroup = 0x7,
    Area = 0x8,
    Guild = 0x9,
    MembersOnly = 0xb,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MessagePayload {
    pub sender_guid: u64,
    pub unknown1: u64,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub sender_first_name: String,
    pub sender_last_name: String,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub target_first_name: String,
    pub target_last_name: String,
    pub message: String,
    pub pos: Pos,
    pub unknown8: u64,
    pub character_type: u32,
}

pub enum SendMessage {
    World(MessagePayload),
    Whisper(MessagePayload),
    System(MessagePayload),
    ReceivedItems(MessagePayload),
    Group(MessagePayload),
    Yell(MessagePayload),
    Trade(MessagePayload),
    LookingForGroup(MessagePayload),
    Area(MessagePayload, u32),
    Guild(MessagePayload),
    MembersOnly(MessagePayload),
}

impl SerializePacket for SendMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            SendMessage::World(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::World as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::Whisper(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Whisper as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::System(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::System as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::ReceivedItems(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::ReceivedItems as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::Group(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Group as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::Yell(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Yell as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::Trade(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Trade as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::LookingForGroup(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::LookingForGroup as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::Area(payload, unknown) => {
                buffer.write_u16::<LittleEndian>(MessageType::Area as u16)?;
                payload.serialize(buffer)?;
                buffer.write_u32::<LittleEndian>(*unknown)?;
                Ok(())
            }
            SendMessage::Guild(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Guild as u16)?;
                payload.serialize(buffer)
            }
            SendMessage::MembersOnly(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::MembersOnly as u16)?;
                payload.serialize(buffer)
            }
        }
    }
}

impl DeserializePacket for SendMessage {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError>
    where
        Self: Sized,
    {
        let raw_message_type = cursor.read_u16::<LittleEndian>()?;
        if let Ok(message_type) = MessageType::try_from(raw_message_type) {
            let payload = MessagePayload::deserialize(cursor)?;
            match message_type {
                MessageType::World => Ok(SendMessage::World(payload)),
                MessageType::Whisper => Ok(SendMessage::Whisper(payload)),
                MessageType::System => Ok(SendMessage::System(payload)),
                MessageType::ReceivedItems => Ok(SendMessage::ReceivedItems(payload)),
                MessageType::Group => Ok(SendMessage::Group(payload)),
                MessageType::Yell => Ok(SendMessage::Yell(payload)),
                MessageType::Trade => Ok(SendMessage::Trade(payload)),
                MessageType::LookingForGroup => Ok(SendMessage::LookingForGroup(payload)),
                MessageType::Area => {
                    let unknown = u32::deserialize(cursor)?;
                    Ok(SendMessage::Area(payload, unknown))
                }
                MessageType::Guild => Ok(SendMessage::Guild(payload)),
                MessageType::MembersOnly => Ok(SendMessage::MembersOnly(payload)),
            }
        } else {
            Err(DeserializePacketError::UnknownDiscriminator)
        }
    }
}

impl GamePacket for SendMessage {
    type Header = ChatOpCode;
    const HEADER: Self::Header = ChatOpCode::SendMessage;
}

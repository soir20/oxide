use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;

use packet_serialize::{
    DeserializePacket, DeserializePacketError, SerializePacket, SerializePacketError,
};

use super::{GamePacket, Name, OpCode, Pos};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum ChatOpCode {
    SendMessage = 0x1,
    SendStringId = 0x4,
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
    Squad = 0x9,
    MembersOnly = 0xb,
}

#[derive(Copy, Clone, Debug)]
pub enum MessageTypeData {
    World,
    Whisper,
    System,
    ReceivedItems,
    Group,
    Yell,
    Trade,
    LookingForGroup,
    Area(u32),
    Squad,
    MembersOnly,
}

impl MessageTypeData {
    pub fn message_type(&self) -> MessageType {
        match self {
            MessageTypeData::World => MessageType::World,
            MessageTypeData::Whisper => MessageType::Whisper,
            MessageTypeData::System => MessageType::System,
            MessageTypeData::ReceivedItems => MessageType::ReceivedItems,
            MessageTypeData::Group => MessageType::Group,
            MessageTypeData::Yell => MessageType::Yell,
            MessageTypeData::Trade => MessageType::Trade,
            MessageTypeData::LookingForGroup => MessageType::LookingForGroup,
            MessageTypeData::Area(_) => MessageType::Area,
            MessageTypeData::Squad => MessageType::Squad,
            MessageTypeData::MembersOnly => MessageType::MembersOnly,
        }
    }
}

#[derive(Clone, SerializePacket, DeserializePacket)]
pub struct MessagePayload {
    pub sender_guid: u64,
    pub target_guid: u64,
    pub channel_name: Name,
    pub target_name: Name,
    pub message: String,
    pub pos: Pos,
    pub squad_guid: u64,
    pub language_id: u32,
}

#[derive(Clone)]
pub struct SendMessage {
    pub message_type_data: MessageTypeData,
    pub payload: MessagePayload,
}

impl SerializePacket for SendMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u16::<LittleEndian>(self.message_type_data.message_type() as u16)?;
        self.payload.serialize(buffer)?;
        match self.message_type_data {
            MessageTypeData::Area(area_id) => {
                Ok::<(), SerializePacketError>(buffer.write_u32::<LittleEndian>(area_id)?)
            }
            _ => Ok(()),
        }?;
        Ok(())
    }
}

impl DeserializePacket for SendMessage {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError>
    where
        Self: Sized,
    {
        let raw_message_type = cursor.read_u16::<LittleEndian>()?;
        let message_type = MessageType::try_from(raw_message_type)
            .map_err(|_| DeserializePacketError::UnknownDiscriminator)?;
        let payload = MessagePayload::deserialize(cursor)?;
        let message_type_data = match message_type {
            MessageType::World => MessageTypeData::World,
            MessageType::Whisper => MessageTypeData::Whisper,
            MessageType::System => MessageTypeData::System,
            MessageType::ReceivedItems => MessageTypeData::ReceivedItems,
            MessageType::Group => MessageTypeData::Group,
            MessageType::Yell => MessageTypeData::Yell,
            MessageType::Trade => MessageTypeData::Trade,
            MessageType::LookingForGroup => MessageTypeData::LookingForGroup,
            MessageType::Area => {
                let area_id = u32::deserialize(cursor)?;
                MessageTypeData::Area(area_id)
            }
            MessageType::Squad => MessageTypeData::Squad,
            MessageType::MembersOnly => MessageTypeData::MembersOnly,
        };

        Ok(SendMessage {
            message_type_data,
            payload,
        })
    }
}

impl GamePacket for SendMessage {
    type Header = ChatOpCode;
    const HEADER: Self::Header = ChatOpCode::SendMessage;
}

#[allow(dead_code)]
#[derive(Copy, Clone, Default)]
pub enum ActionBarTextColor {
    #[default]
    White = 0,
    Red = 1,
    Yellow = 2,
    Green = 3,
    Blue = 4,
}

impl SerializePacket for ActionBarTextColor {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct SendStringId {
    pub sender_guid: u64,
    pub message_id: u32,
    pub is_anonymous: bool,
    pub unknown2: bool,
    pub is_action_bar_message: bool,
    pub action_bar_text_color: ActionBarTextColor,
    pub target_guid: u64,
    pub owner_guid: u64,
    pub unknown7: u32,
}

impl GamePacket for SendStringId {
    type Header = ChatOpCode;

    const HEADER: Self::Header = ChatOpCode::SendStringId;
}

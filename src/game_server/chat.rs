use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;

use packet_serialize::{DeserializePacket, DeserializePacketError, SerializePacket, SerializePacketError};

use crate::game_server::{Broadcast, ProcessPacketError};
use crate::game_server::character_guid::player_guid;
use crate::game_server::game_packet::{GamePacket, OpCode, Pos};
use crate::game_server::tunnel::TunneledPacket;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum ChatOpCode {
    SendMessage              = 0x1
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
    World                    = 0x0,
    Whisper                  = 0x1,
    System                   = 0x2,
    ReceivedItems            = 0x3,
    Group                    = 0x4,
    Yell                     = 0x5,
    Trade                    = 0x6,
    LookingForGroup          = 0x7,
    Area                     = 0x8,
    Guild                    = 0x9,
    MembersOnly              = 0xb
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MessagePayload {
    sender_guid: u64,
    unknown1: u64,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    sender_first_name: String,
    sender_last_name: String,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
    target_first_name: String,
    target_last_name: String,
    message: String,
    pos: Pos,
    unknown8: u64,
    character_type: u32,
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
    MembersOnly(MessagePayload)
}

impl SerializePacket for SendMessage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            SendMessage::World(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::World as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::Whisper(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Whisper as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::System(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::System as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::ReceivedItems(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::ReceivedItems as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::Group(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Group as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::Yell(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Yell as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::Trade(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Trade as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::LookingForGroup(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::LookingForGroup as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::Area(payload, unknown) => {
                buffer.write_u16::<LittleEndian>(MessageType::Area as u16)?;
                payload.serialize(buffer)?;
                buffer.write_u32::<LittleEndian>(*unknown)?;
                Ok(())
            },
            SendMessage::Guild(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::Guild as u16)?;
                payload.serialize(buffer)
            },
            SendMessage::MembersOnly(payload) => {
                buffer.write_u16::<LittleEndian>(MessageType::MembersOnly as u16)?;
                payload.serialize(buffer)
            }
        }
    }
}

impl DeserializePacket for SendMessage {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError> where Self: Sized {
        let raw_message_type =cursor.read_u16::<LittleEndian>()?;
        if let Ok(message_type) =  MessageType::try_from(raw_message_type) {
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
                },
                MessageType::Guild => Ok(SendMessage::Guild(payload)),
                MessageType::MembersOnly => Ok(SendMessage::MembersOnly(payload))
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

pub fn process_chat_packet(cursor: &mut Cursor<&[u8]>, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match ChatOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            ChatOpCode::SendMessage => {
                let message = SendMessage::deserialize(cursor)?;
                Ok(vec![
                    Broadcast::Single(
                        sender,
                        vec![
                            GamePacket::serialize(
                                &TunneledPacket {
                                    unknown1: true,
                                    inner: match message {
                                        SendMessage::World(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::World(payload)
                                        },
                                        SendMessage::Whisper(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::Whisper(payload)
                                        },
                                        SendMessage::System(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::System(payload)
                                        },
                                        SendMessage::ReceivedItems(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::ReceivedItems(payload)
                                        },
                                        SendMessage::Group(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::Group(payload)
                                        },
                                        SendMessage::Yell(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::Yell(payload)
                                        },
                                        SendMessage::Trade(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::Trade(payload)
                                        },
                                        SendMessage::LookingForGroup(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::LookingForGroup(payload)
                                        },
                                        SendMessage::Area(mut payload, unknown) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::Area(payload, unknown)
                                        },
                                        SendMessage::Guild(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::Guild(payload)
                                        },
                                        SendMessage::MembersOnly(mut payload) => {
                                            payload.sender_guid = player_guid(sender);
                                            SendMessage::MembersOnly(payload)
                                        },
                                    },
                                }
                            )?
                        ]
                    )
                ])
            }
        },
        Err(_) => {
            println!("Unknown chat op code: {}", raw_op_code);
            Err(ProcessPacketError::CorruptedPacket)
        }
    }
}

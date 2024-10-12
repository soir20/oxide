use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::DeserializePacket;

use crate::game_server::{
    packets::{
        chat::{ChatOpCode, SendMessage},
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, ProcessPacketError, ProcessPacketErrorType,
};

use super::unique_guid::player_guid;

pub fn process_chat_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match ChatOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            ChatOpCode::SendMessage => {
                let message = SendMessage::deserialize(cursor)?;
                Ok(vec![Broadcast::Single(
                    sender,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: match message {
                            SendMessage::World(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::World(payload)
                            }
                            SendMessage::Whisper(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::Whisper(payload)
                            }
                            SendMessage::System(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::System(payload)
                            }
                            SendMessage::ReceivedItems(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::ReceivedItems(payload)
                            }
                            SendMessage::Group(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::Group(payload)
                            }
                            SendMessage::Yell(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::Yell(payload)
                            }
                            SendMessage::Trade(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::Trade(payload)
                            }
                            SendMessage::LookingForGroup(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::LookingForGroup(payload)
                            }
                            SendMessage::Area(mut payload, unknown) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::Area(payload, unknown)
                            }
                            SendMessage::Guild(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::Guild(payload)
                            }
                            SendMessage::MembersOnly(mut payload) => {
                                payload.sender_guid = player_guid(sender);
                                SendMessage::MembersOnly(payload)
                            }
                        },
                    })?],
                )])
            }
        },
        Err(_) => Err(ProcessPacketError::new(
            ProcessPacketErrorType::UnknownOpCode,
            format!("Unknown chat op code: {}", raw_op_code),
        )),
    }
}

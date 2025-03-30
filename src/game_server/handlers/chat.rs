use std::io::{Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::DeserializePacket;

use crate::game_server::{
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    packets::{
        GamePacket,
        chat::{ChatOpCode, MessageTypeData, SendMessage},
        tunnel::TunneledPacket,
    },
};

use super::{
    guid::GuidTableIndexer,
    lock_enforcer::CharacterLockRequest,
    unique_guid::{player_guid, shorten_player_guid},
    zone::ZoneInstance,
};

pub fn process_chat_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match ChatOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            ChatOpCode::SendMessage => {
                game_server
                    .lock_enforcer()
                    .read_characters(|_| CharacterLockRequest {
                        read_guids: Vec::new(),
                        write_guids: Vec::new(),
                        character_consumer: |characters_table_read_handle, _, _, _| {
                            let mut message = SendMessage::deserialize(cursor)?;
                            message.payload.sender_guid = player_guid(sender);
                            message.payload.channel_name.first_name = characters_table_read_handle
                                .index2(player_guid(sender))
                                .cloned()
                                .ok_or_else(|| {
                                    ProcessPacketError::new(
                                        ProcessPacketErrorType::ConstraintViolated,
                                        format!("Unknown player {} sent chat message", sender),
                                    )
                                })?;
                            message.payload.channel_name.last_name = String::default();

                            match message.message_type_data {
                                MessageTypeData::World => {
                                    let (_, instance_guid, chunk) = characters_table_read_handle
                                        .index1(message.payload.sender_guid)
                                        .ok_or_else(|| {
                                            ProcessPacketError::new(
                                                ProcessPacketErrorType::ConstraintViolated,
                                                format!(
                                                    "Unknown player {} sent world chat message",
                                                    sender
                                                ),
                                            )
                                        })?;
                                    let all_players_nearby = ZoneInstance::all_players_nearby(
                                        chunk,
                                        instance_guid,
                                        characters_table_read_handle,
                                    )?;
                                    Ok(vec![Broadcast::Multi(
                                        all_players_nearby,
                                        vec![GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: message,
                                        })?],
                                    )])
                                }
                                MessageTypeData::Whisper => {
                                    if message.payload.channel_name.first_name
                                        == message.payload.target_name.first_name
                                    {
                                        return Ok(Vec::new());
                                    }

                                    let message_packet_for_target =
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: message.clone(),
                                        })?;
                                    let mut broadcasts: Vec<Broadcast> =
                                        characters_table_read_handle
                                            .keys_by_index2(&message.payload.target_name.first_name)
                                            .filter_map(|guid| shorten_player_guid(guid).ok())
                                            .map(|target_guid| {
                                                Broadcast::Single(
                                                    target_guid,
                                                    vec![message_packet_for_target.clone()],
                                                )
                                            })
                                            .collect();

                                    // Don't send any response if the character being messaged doesn't exist
                                    if broadcasts.is_empty() {
                                        return Ok(broadcasts);
                                    }

                                    // We also need to send the chat to the sender so they see their own messages
                                    message.payload.channel_name =
                                        message.payload.target_name.clone();

                                    // Required for the UI to properly display our own messages
                                    message.payload.message = format!(
                                        "You whisper to [{}]:{}",
                                        message.payload.target_name, message.payload.message
                                    );

                                    let message_packet_for_sender =
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: message,
                                        })?;
                                    broadcasts.push(Broadcast::Single(
                                        sender,
                                        vec![message_packet_for_sender],
                                    ));

                                    Ok(broadcasts)
                                }
                                MessageTypeData::Squad => {
                                    if let Some(squad_guid) =
                                        characters_table_read_handle.index3(player_guid(sender))
                                    {
                                        message.payload.squad_guid = *squad_guid;

                                        let players_in_squad = characters_table_read_handle
                                            .keys_by_index3(squad_guid)
                                            .filter_map(|guid| shorten_player_guid(guid).ok())
                                            .collect();

                                        Ok(vec![Broadcast::Multi(
                                            players_in_squad,
                                            vec![GamePacket::serialize(&TunneledPacket {
                                                unknown1: true,
                                                inner: message,
                                            })?],
                                        )])
                                    } else {
                                        Ok(Vec::new())
                                    }
                                }
                                _ => Ok(Vec::new()),
                            }
                        },
                    })
            }
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unimplemented chat op code: {:?}, {:x?}", op_code, buffer),
                ))
            }
        },
        Err(_) => Err(ProcessPacketError::new(
            ProcessPacketErrorType::UnknownOpCode,
            format!("Unknown chat op code: {}", raw_op_code),
        )),
    }
}

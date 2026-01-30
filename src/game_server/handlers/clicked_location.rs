use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::compute_destination_rot,
    packets::clicked_location::{ClickedLocationOpCode, ClickedLocationRequest},
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::{
    character::{coerce_to_broadcast_supplier, CharacterType},
    lock_enforcer::CharacterLockRequest,
    unique_guid::player_guid,
    zone::teleport_within_zone,
    WriteLockingBroadcastSupplier,
};

pub fn process_clicked_location(
    game_server: &GameServer,
    sender: u32,
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u8 = DeserializePacket::deserialize(cursor)?;
    match ClickedLocationOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            ClickedLocationOpCode::ClickedLocationRequest => {
                let clicked_location = ClickedLocationRequest::deserialize(cursor)?;
                let requester_guid = player_guid(sender);

                let broadcast_supplier: WriteLockingBroadcastSupplier = game_server
                    .lock_enforcer()
                    .read_characters(|_| CharacterLockRequest {
                        read_guids: Vec::new(),
                        write_guids: vec![requester_guid],
                        character_consumer: move |_, _, characters_write, _| {
                            let Some(requester_read_handle) = characters_write.get(&requester_guid) else {
                                return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                            };

                            let player_stats = match &requester_read_handle.stats.character_type {
                                CharacterType::Player(player) => player.as_ref(),
                                _ => {
                                    return coerce_to_broadcast_supplier(move |_| {
                                        Err(ProcessPacketError::new(
                                            ProcessPacketErrorType::ConstraintViolated,
                                            format!(
                                                "Requester {} sent a ClickedLocationRequest but is not a player",
                                                requester_guid
                                            ),
                                        ))
                                    });
                                }
                            };

                            let destination_rot = compute_destination_rot(clicked_location.current_pos, clicked_location.clicked_pos);

                            if player_stats.toggles.click_to_teleport {
                                coerce_to_broadcast_supplier(move |_| {
                                    Ok(teleport_within_zone(sender, clicked_location.clicked_pos, destination_rot))
                                })
                            } else {
                                coerce_to_broadcast_supplier(move |_| {
                                    Ok(vec![Broadcast::Single(sender, vec![])])
                                })
                            }
                        },
                    });

                broadcast_supplier?(game_server)
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown clicked location packet: {raw_op_code}, {buffer:x?}"),
            ))
        }
    }
}

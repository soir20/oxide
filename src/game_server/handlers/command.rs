use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    packets::command::{AdvanceDialog, CommandOpCode, InteractRequest},
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::{
    character::{coerce_to_broadcast_supplier, CharacterType},
    dialog::handle_dialog_buttons,
    lock_enforcer::{CharacterLockRequest, ZoneLockEnforcer, ZoneLockRequest},
    unique_guid::player_guid,
    zone::interact_with_character,
    WriteLockingBroadcastSupplier,
};

pub fn process_command(
    game_server: &GameServer,
    sender: u32,
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u16 = DeserializePacket::deserialize(cursor)?;
    match CommandOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            CommandOpCode::SelectPlayer => Ok(Vec::new()),
            CommandOpCode::InteractRequest => {
                let req = InteractRequest::deserialize(cursor)?;
                interact_with_character(player_guid(sender), req.target, game_server)
            }
            CommandOpCode::AdvanceDialog => {
                let advancement = AdvanceDialog::deserialize(cursor)?;
                let requester_guid = player_guid(sender);

                let broadcast_supplier: WriteLockingBroadcastSupplier = game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![requester_guid],
            character_consumer: move |_, _, mut characters_write, minigame_data_lock_enforcer| {
                let Some(character) = characters_write.get_mut(&requester_guid) else {
                    return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                };

                let instance_guid = character.stats.instance_guid;

                let player_stats = match &mut character.stats.character_type {
                    CharacterType::Player(player) => player.as_mut(),
                    _ => {
                        return coerce_to_broadcast_supplier(move |_| Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "(Requester: {}) tried to advance dialog but is a non-player character", requester_guid
                            ),
                        )));
                    }
                };

                let zones_lock_enforcer: ZoneLockEnforcer = minigame_data_lock_enforcer.into();

                zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                    read_guids: vec![instance_guid],
                    write_guids: Vec::new(),
                    zone_consumer: move |_, zones_read, _| {
                        let Some(zone_instance) = zones_read.get(&instance_guid) else {
                            return coerce_to_broadcast_supplier(move |_| Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!(
                                    "(Requester: {}) tried to select (Button ID: {}) but is in a non-existent zone",
                                    requester_guid, advancement.button_id
                                ),
                            )));
                        };

                        let result = handle_dialog_buttons(
                            sender,
                            advancement.button_id,
                            player_stats,
                            zone_instance,
                            game_server,
                        )
                        .map(|packets| Broadcast::Single(sender, packets));

                        coerce_to_broadcast_supplier(move |_| Ok(vec![result?]))
                    },
                })
            },
        });

                broadcast_supplier?(game_server)
            }
            // Ignore this packet to reduce log spam
            CommandOpCode::ExitDialog => Ok(Vec::new()),
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unimplemented command packet: {op_code:?}, {buffer:x?}"),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown command packet: {raw_op_code}, {buffer:x?}"),
            ))
        }
    }
}

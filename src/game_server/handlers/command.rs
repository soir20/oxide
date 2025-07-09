use std::io::{Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::DeserializePacket;

use crate::game_server::{
    packets::command::{CommandOpCode, InteractRequest},
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::{unique_guid::player_guid, zone::interact_with_character};

pub fn process_command(
    game_server: &GameServer,
    sender: u32,
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match CommandOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            CommandOpCode::SelectPlayer => Ok(Vec::new()),
            CommandOpCode::InteractRequest => {
                let req = InteractRequest::deserialize(cursor)?;
                interact_with_character(player_guid(sender), req.target, game_server)
            }
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

use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    packets::command::{AdvanceDialog, CommandOpCode, InteractRequest},
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::{
    dialog::handle_dialog_buttons, unique_guid::player_guid, zone::interact_with_character,
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
                handle_dialog_buttons(sender, advancement.button_id, game_server)
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

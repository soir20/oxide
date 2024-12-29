use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::DeserializePacket;

use crate::{
    game_server::{
        packets::command::{AdvanceDialog, CommandOpCode, ExitDialog, SelectPlayer},
        Broadcast, GameServer, ProcessPacketError,
    },
    info,
};

use super::zone::interact_with_character;

pub fn process_command(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match CommandOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            CommandOpCode::SelectPlayer => {
                let req = SelectPlayer::deserialize(cursor)?;
                interact_with_character(req, game_server)
            }
            CommandOpCode::AdvanceDialog => {
                let advance_dialog = AdvanceDialog::deserialize(cursor)?;

                info!("Received Button ID {}", advance_dialog.button_id);
                Ok(Vec::new())
            }
            _ => {
                info!("Unimplemented command: {:?}", op_code);
                Ok(Vec::new())
            }
        },
        Err(_) => {
            info!("Unknown command: {}", raw_op_code);
            Ok(Vec::new())
        }
    }
}

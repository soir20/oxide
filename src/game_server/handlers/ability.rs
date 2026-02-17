use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    packets::ability::AbilityOpCode, Broadcast, ProcessPacketError,
    ProcessPacketErrorType,
};

pub fn process_ability(cursor: &mut Cursor<&[u8]>) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u16 = DeserializePacket::deserialize(cursor)?;
    match AbilityOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            // Ability definitions are presumably unused, so ignore
            AbilityOpCode::RequestAbilityDefinition => Ok(Vec::new()),
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unimplemented ability packet: {op_code:?}, {buffer:x?}"),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown ability packet: {raw_op_code}, {buffer:x?}"),
            ))
        }
    }
}

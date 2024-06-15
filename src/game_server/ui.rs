use byteorder::WriteBytesExt;

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::game_packet::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum UiOpCode {
    ExecuteScriptWithParams = 0x8,
}

impl SerializePacket for UiOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Ui.serialize(buffer)?;
        buffer.write_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ExecuteScriptWithParams {
    pub script_name: String,
    pub params: Vec<String>,
}

impl GamePacket for ExecuteScriptWithParams {
    type Header = UiOpCode;
    const HEADER: Self::Header = UiOpCode::ExecuteScriptWithParams;
}

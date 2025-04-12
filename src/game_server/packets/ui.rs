use byteorder::WriteBytesExt;

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum UiOpCode {
    ExecuteScriptWithIntParams = 0x7,
    ExecuteScriptWithStringParams = 0x8,
}

impl SerializePacket for UiOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Ui.serialize(buffer)?;
        buffer.write_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ExecuteScriptWithIntParams {
    pub script_name: String,
    pub params: Vec<i32>,
}

impl GamePacket for ExecuteScriptWithIntParams {
    type Header = UiOpCode;
    const HEADER: Self::Header = UiOpCode::ExecuteScriptWithIntParams;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ExecuteScriptWithStringParams {
    pub script_name: String,
    pub params: Vec<String>,
}

impl GamePacket for ExecuteScriptWithStringParams {
    type Header = UiOpCode;
    const HEADER: Self::Header = UiOpCode::ExecuteScriptWithStringParams;
}

use num_enum::TryFromPrimitive;

use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode, Pos};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ClickedLocationOpCode {
    ClickedLocationRequest = 0x1,
}

impl SerializePacket for ClickedLocationOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::ClickedLocation.serialize(buffer);
        (*self as u8).serialize(buffer);
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ClickedLocationRequest {
    pub guid: u64,
    pub unknown1: u32,
    pub unknown2: u32,
    pub current_pos: Pos,
    pub clicked_pos: Pos,
    pub unknown3: u32,
}

impl GamePacket for ClickedLocationRequest {
    type Header = ClickedLocationOpCode;
    const HEADER: Self::Header = ClickedLocationOpCode::ClickedLocationRequest;
}

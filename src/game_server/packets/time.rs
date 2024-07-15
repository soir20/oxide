use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(SerializePacket, DeserializePacket)]
pub struct GameTimeSync {
    pub time: u64,
    pub unknown1: u32,
    pub unknown2: bool,
}

impl GamePacket for GameTimeSync {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::GameTimeSync;
}

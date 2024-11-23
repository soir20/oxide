use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(SerializePacket, DeserializePacket)]
pub struct GameTimeSync {
    pub time: u64,
    pub cycles_per_day: u32,
    pub keep_client_time: bool,
}

impl GamePacket for GameTimeSync {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::GameTimeSync;
}

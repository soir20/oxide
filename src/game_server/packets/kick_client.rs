use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(SerializePacket, DeserializePacket)]
pub struct KickClient {
    pub reason: String,
}

impl GamePacket for KickClient {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::KickClient;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ScheduledClientKick {
    pub reason: String,
    pub seconds_remaining: u32,
}

impl GamePacket for ScheduledClientKick {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::ScheduledClientKick;
}

use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(SerializePacket, DeserializePacket)]
pub struct ClientMetrics {
    pub time: u64,
    pub unknown1: u64,
    pub unknown2: u64,
    pub unknown3: u64,
    pub unknown4: u64,
    pub unknown5: u64,
    pub unknown6: u64,
    pub unknown7: u64,
    pub unknown8: u64,
    pub unknown9: u64,
    pub unknown10: u64,
    pub unknown11: u64,
    pub render_level: u32,
    pub window_width: u64,
    pub window_height: u64,
    pub working_set_memory_bytes: u64,
    pub average_ping: u64,
    pub idle_seconds: u64,
    pub idle_level: u64,
}

impl GamePacket for ClientMetrics {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::ClientMetrics;
}

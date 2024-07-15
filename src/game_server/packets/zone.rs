use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneTeleportRequest {
    pub destination_guid: u32,
}

impl GamePacket for ZoneTeleportRequest {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::ZoneTeleportRequest;
}

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

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneCombatSettings {
    pub zone_guid: u32,
    pub combat_pose: bool,
    pub combat_camera: bool,
    pub unknown3: bool,
    pub unknown4: bool,
    pub unknown5: u32,
}

impl GamePacket for ZoneCombatSettings {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::ZoneCombatSettings;
}

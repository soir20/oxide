use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(SerializePacket, DeserializePacket)]
pub struct PointOfInterestTeleportRequest {
    pub point_of_interest_guid: u32,
}

impl GamePacket for PointOfInterestTeleportRequest {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::PointOfInterestTeleportRequest;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneCombatSettings {
    pub zone_guid: u32,
    pub combat_camera: bool,
    pub unknown3: bool,
    pub unknown4: bool,
    pub unknown5: u32,
}

impl GamePacket for ZoneCombatSettings {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::ZoneCombatSettings;
}

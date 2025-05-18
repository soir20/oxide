use packet_serialize::{DeserializePacket, NullTerminatedString, SerializePacket};

use crate::game_server::handlers::zone::PointOfInterestConfig;

use super::{GamePacket, OpCode, Pos};

#[derive(DeserializePacket)]
pub struct LoginRequest {
    pub ticket: String,
    pub guid: u64,
    pub version: String,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct LoginReply {
    pub logged_in: bool,
}

impl GamePacket for LoginReply {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::LoginReply;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct DeploymentEnv {
    pub environment: NullTerminatedString,
}

impl GamePacket for DeploymentEnv {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::DeploymentEnv;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneDetails {
    pub name: String,
    pub zone_type: u32,
    pub hide_ui: bool,
    pub combat_hud: bool,
    pub sky_definition_file_name: String,
    pub combat_camera: bool,
    pub unknown7: u32,
    pub unknown8: u32,
}

impl GamePacket for ZoneDetails {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::ZoneDetails;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct GameSettings {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: bool,
    pub time_scale: f32,
}

impl GamePacket for GameSettings {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::ClientGameSettings;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct WelcomeScreenUnknown1 {}

#[derive(SerializePacket, DeserializePacket)]
pub struct WelcomeScreenUnknown2 {}

#[derive(SerializePacket, DeserializePacket)]
pub struct WelcomeScreen {
    pub show_ui: bool,
    pub unknown1: Vec<WelcomeScreenUnknown1>,
    pub unknown2: Vec<WelcomeScreenUnknown2>,
    pub unknown3: u32,
    pub unknown4: u32,
}

impl GamePacket for WelcomeScreen {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::WelcomeScreen;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneDetailsDone {}

impl GamePacket for ZoneDetailsDone {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::ZoneDetailsDone;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Logout {}

impl GamePacket for Logout {
    type Header = OpCode;

    const HEADER: Self::Header = OpCode::Logout;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ClientBeginZoning {
    pub zone_name: String,
    pub zone_type: u32,
    pub pos: Pos,
    pub rot: Pos,
    pub sky_definition_file_name: String,
    pub unknown1: bool,
    pub zone_id: u8,
    pub zone_name_id: u32,
    pub world_id: u32,
    pub world_name_id: u32,
    pub unknown6: bool,
    pub unknown7: bool,
}

impl GamePacket for ClientBeginZoning {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::ClientBeginZoning;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PointOfInterest {
    pub guid: u32,
    pub name_id: u32,
    pub unknown1: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub heading: f32,
    pub icon_id: u32,
    pub notification_type: u32,
    pub subtitle_id: u32,
    pub quest_id: u32,
    pub unknown2: u32,
    pub zone_template_guid: u32,
}

impl From<&(u8, PointOfInterestConfig)> for PointOfInterest {
    fn from((zone_template_guid, value): &(u8, PointOfInterestConfig)) -> Self {
        PointOfInterest {
            guid: value.guid,
            name_id: value.name_id,
            unknown1: 0,
            x: value.pos.x,
            y: value.pos.y,
            z: value.pos.z,
            heading: 0.0,
            icon_id: 0,
            notification_type: 0,
            subtitle_id: 0,
            quest_id: 0,
            unknown2: 0,
            zone_template_guid: *zone_template_guid as u32,
        }
    }
}

pub struct DefinePointsOfInterest {
    pub points: Vec<PointOfInterest>,
}

impl SerializePacket for DefinePointsOfInterest {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let mut inner_buffer = Vec::new();

        for point in self.points.iter() {
            1u8.serialize(&mut inner_buffer);
            point.serialize(&mut inner_buffer);
        }
        0u8.serialize(&mut inner_buffer);

        inner_buffer.serialize(buffer);
    }
}

impl GamePacket for DefinePointsOfInterest {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::DefinePointsOfInterest;
}

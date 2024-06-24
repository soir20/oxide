use byteorder::{LittleEndian, WriteBytesExt};
use std::io::Write;

use packet_serialize::{
    DeserializePacket, NullTerminatedString, SerializePacket, SerializePacketError,
};

use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, Pos, StringId};
use crate::game_server::guid::GuidTableHandle;
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::GameServer;

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
    pub zoom_out: bool,
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
    pub id: u32,
    pub name_id: StringId,
    pub location_id: u32,
    pub teleport_pos: Pos,
    pub icon_id: ImageId,
    pub notification_type: u32,
    pub subtitle_id: StringId,
    pub unknown: u32,
    pub quest_id: u32,
    pub teleport_pos_id: u32,
}

pub struct DefinePointsOfInterest {
    points: Vec<PointOfInterest>,
}

impl SerializePacket for DefinePointsOfInterest {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();

        for point in self.points.iter() {
            inner_buffer.write_u8(1)?;
            SerializePacket::serialize(point, &mut inner_buffer)?;
        }
        inner_buffer.write_u8(0)?;

        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;

        Ok(())
    }
}

impl GamePacket for DefinePointsOfInterest {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::DefinePointsOfInterest;
}

pub fn send_points_of_interest(
    game_server: &GameServer,
) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    let mut points = Vec::new();
    for (_, zone) in game_server.read_zones().iter() {
        let zone_read_handle = zone.read();
        points.push(PointOfInterest {
            id: zone_read_handle.template_guid as u32,
            name_id: 0,
            location_id: 0,
            teleport_pos: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            icon_id: 0,
            notification_type: 0,
            subtitle_id: 0,
            unknown: 0,
            quest_id: 0,
            teleport_pos_id: 0,
        });
    }

    Ok(vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: DefinePointsOfInterest { points },
    })?])
}

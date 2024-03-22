use std::io::Error;
use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, NullTerminatedString, SerializePacket};
use crate::game_server::game_packet::{GamePacket, OpCode};

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
    pub environment: NullTerminatedString
}

impl GamePacket for DeploymentEnv {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::DeploymentEnv;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneDetails {
    pub name: String,
    pub id: u32,
    pub hide_ui: bool,
    pub direction_indicator: bool,
    pub sky_definition_file_name: String,
    pub zoom_out: bool,
    pub unknown7: u32,
    pub unknown8: u32
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
    pub unknown5: f32
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

pub fn send_item_definitions() -> Result<Vec<u8>, Error> {
    let mut bytes: Vec<u8> = vec![];
    let mut buffer = Vec::new();
    buffer.write_u16::<LittleEndian>(0x25)?;
    buffer.write_i32::<LittleEndian>(bytes.len() as i32)?;
    buffer.append(&mut bytes);
    //make_tunneled_packet(0x23, &buffer)
    Ok(buffer)
}

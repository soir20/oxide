use byteorder::{LittleEndian, WriteBytesExt};
use serde::Deserialize;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

#[derive(Copy, Clone, Debug)]
pub enum OpCode {
    LoginRequest             = 0x1,
    LoginReply               = 0x2,
    TunneledClient           = 0x5,
    Player                   = 0xc,
    ClientIsReady            = 0xd,
    ZoneDetailsDone          = 0xe,
    Command                  = 0x1a,
    ClientBeginZoning        = 0x1f,
    PlayerUpdate             = 0x23,
    ClientUpdate             = 0x26,
    ZoneDetails              = 0x2b,
    GameTimeSync             = 0x34,
    WelcomeScreen            = 0x5d,
    ClientGameSettings       = 0x8f,
    DeploymentEnv            = 0xa5,
    Portrait                 = 0x9b
}

pub struct UnknownOpCode;

impl TryFrom<u16> for OpCode {
    type Error = UnknownOpCode;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x1 => Ok(OpCode::LoginRequest),
            0x2 => Ok(OpCode::LoginReply) ,
            0x5 => Ok(OpCode::TunneledClient),
            0xc => Ok(OpCode::Player),
            0xd => Ok(OpCode::ClientIsReady),
            0xe => Ok(OpCode::ZoneDetailsDone),
            0x1a => Ok(OpCode::Command),
            0x1f => Ok(OpCode::ClientBeginZoning),
            0x23 => Ok(OpCode::PlayerUpdate),
            0x26 => Ok(OpCode::ClientUpdate),
            0x2b => Ok(OpCode::ZoneDetails),
            0x34 => Ok(OpCode::GameTimeSync),
            0x5d => Ok(OpCode::WelcomeScreen),
            0x8f => Ok(OpCode::ClientGameSettings),
            0xa5 => Ok(OpCode::DeploymentEnv),
            0x9b => Ok(OpCode::Portrait),
            _ => Err(UnknownOpCode)
        }
    }
}

impl SerializePacket for OpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub trait GamePacket: SerializePacket {
    type Header: SerializePacket;
    const HEADER: Self::Header;

    fn serialize(&self) -> Result<Vec<u8>, SerializePacketError> {
        let mut buffer = Vec::new();
        SerializePacket::serialize(&Self::HEADER, &mut buffer)?;
        SerializePacket::serialize(self, &mut buffer)?;
        Ok(buffer)
    }
}

#[derive(Copy, Clone, SerializePacket, DeserializePacket, Deserialize)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Effect {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: bool,
    pub unknown9: u64,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub unknown13: u32,
    pub unknown14: u64,
    pub unknown15: u32,
    pub unknown16: u32,
    pub unknown17: bool,
    pub unknown18: bool,
    pub unknown19: bool,
}

pub type StringId = u32;
pub type ImageId = u32;

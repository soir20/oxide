use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use serde::Deserialize;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum OpCode {
    LoginRequest             = 0x1,
    LoginReply               = 0x2,
    TunneledClient           = 0x5,
    TunneledWorld            = 0x6,
    Player                   = 0xc,
    ClientIsReady            = 0xd,
    ZoneDetailsDone          = 0xe,
    Command                  = 0x1a,
    ClientBeginZoning        = 0x1f,
    Combat                   = 0x20,
    PlayerUpdate             = 0x23,
    ClientUpdate             = 0x26,
    ZoneDetails              = 0x2b,
    Ui                       = 0x2f,
    GameTimeSync             = 0x34,
    DefinePointsOfInterest   = 0x39,
    ZoneTeleportRequest      = 0x5a,
    WelcomeScreen            = 0x5d,
    TeleportToSafety         = 0x7a,
    UpdatePlayerPosition     = 0x7d,
    Housing                  = 0x7f,
    ClientGameSettings       = 0x8f,
    Portrait                 = 0x9b,
    Mount                    = 0xa7,
    Store                    = 0xa4,
    DeploymentEnv            = 0xa5,
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

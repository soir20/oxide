use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};

#[derive(Copy, Clone, Debug)]
pub enum OpCode {
    LoginRequest             = 0x1,
    LoginReply               = 0x2,
    TunneledClient           = 0x5,
    Player                   = 0xc,
    ClientIsReady            = 0xd,
    ZoneDetailsDone          = 0xe,
    PlayerUpdate             = 0x23,
    ClientUpdate             = 0x26,
    ZoneDetails              = 0x2b,
    GameTimeSync             = 0x34,
    WelcomeScreen            = 0x5d,
    ClientGameSettings       = 0x8f,
    DeploymentEnv            = 0xa5,
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
            0x23 => Ok(OpCode::PlayerUpdate),
            0x26 => Ok(OpCode::ClientUpdate),
            0x2b => Ok(OpCode::ZoneDetails),
            0x34 => Ok(OpCode::GameTimeSync),
            0x5d => Ok(OpCode::WelcomeScreen),
            0x8f => Ok(OpCode::ClientGameSettings),
            0xa5 => Ok(OpCode::DeploymentEnv),
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

#[derive(SerializePacket)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rot: f32
}

#[derive(SerializePacket)]
pub struct Effect {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
    unknown8: bool,
    unknown9: u64,
    unknown10: u32,
    unknown11: u32,
    unknown12: u32,
    unknown13: u32,
    unknown14: u64,
    unknown15: u32,
    unknown16: u32,
    unknown17: bool,
    unknown18: bool,
    unknown19: bool,
}

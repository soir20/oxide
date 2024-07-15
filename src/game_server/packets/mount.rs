use byteorder::WriteBytesExt;
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MountOpCode {
    MountRequest = 0x1,
    MountReply = 0x2,
    DismountRequest = 0x3,
    DismountReply = 0x4,
    MountList = 0x5,
    MountSpawn = 0x6,
    MountSpawnByItemDef = 0x8,
    MountListShowMarket = 0x9,
    SetAutoMount = 0xa,
}

impl SerializePacket for MountOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Mount.serialize(buffer)?;
        buffer.write_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct DismountReply {
    pub rider_guid: u64,
    pub composite_effect: u32,
}

impl GamePacket for DismountReply {
    type Header = MountOpCode;
    const HEADER: Self::Header = MountOpCode::DismountReply;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MountReply {
    pub rider_guid: u64,
    pub mount_guid: u64,
    pub seat: u32,
    pub queue_pos: u32,
    pub unknown3: u32,
    pub composite_effect: u32,
    pub unknown5: u32,
}

impl GamePacket for MountReply {
    type Header = MountOpCode;
    const HEADER: Self::Header = MountOpCode::MountReply;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MountSpawn {
    pub mount_id: u32,
}

impl GamePacket for MountSpawn {
    type Header = MountOpCode;
    const HEADER: Self::Header = MountOpCode::MountSpawn;
}

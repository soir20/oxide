use std::io::Cursor;

use byteorder::{ReadBytesExt, WriteBytesExt};

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::{Broadcast, ProcessPacketError};
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::tunnel::TunneledPacket;

#[derive(Copy, Clone, Debug)]
pub enum MountOpCode {
    MountRequest             = 0x1,
    MountReply               = 0x2,
    DismountRequest          = 0x3,
    DismountReply            = 0x4,
    MountList                = 0x5,
    MountSpawn               = 0x6,
    MountSpawnByItemDef      = 0x8,
    MountListShowMarket      = 0x9,
    SetAutoMount             = 0xa
}

impl SerializePacket for MountOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Mount.serialize(buffer)?;
        buffer.write_u8(*self as u8)?;
        Ok(())
    }
}

pub struct UnknownMountOpCode;

impl TryFrom<u8> for MountOpCode {
    type Error = UnknownMountOpCode;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x1 => Ok(MountOpCode::MountRequest),
            0x2 => Ok(MountOpCode::MountReply),
            0x3 => Ok(MountOpCode::DismountRequest),
            0x4 => Ok(MountOpCode::DismountReply),
            0x5 => Ok(MountOpCode::MountList),
            0x6 => Ok(MountOpCode::MountSpawn),
            0x8 => Ok(MountOpCode::MountSpawnByItemDef),
            0x9 => Ok(MountOpCode::MountListShowMarket),
            0xa => Ok(MountOpCode::SetAutoMount),
            _ => Err(UnknownMountOpCode)
        }
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct DismountReply {
    rider_guid: u64,
    composite_effect: u32
}

impl GamePacket for DismountReply {
    type Header = MountOpCode;
    const HEADER: Self::Header = MountOpCode::DismountReply;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MountReply {
    rider_guid: u64,
    mount_guid: u64,
    unknown1: u32,
    queue_pos: u32,
    unknown3: u32,
    composite_effect: u32,
    unknown5: u32
}

impl GamePacket for MountReply {
    type Header = MountOpCode;
    const HEADER: Self::Header = MountOpCode::MountReply;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MountSpawn {
    mount_id: u32
}

impl GamePacket for MountSpawn {
    type Header = MountOpCode;
    const HEADER: Self::Header = MountOpCode::MountSpawn;
}

pub fn handle_mount_packet(cursor: &mut Cursor<&[u8]>, sender: u64) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u8()?;
    match MountOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            MountOpCode::DismountRequest => Ok(vec![
                Broadcast::Single(sender, vec![
                    GamePacket::serialize(
                        &TunneledPacket {
                            unknown1: true,
                            inner: DismountReply {
                                rider_guid: sender,
                                composite_effect: 0,
                            },
                        }
                    )?
                ])
            ]),
            MountOpCode::MountSpawn => Ok(vec![
                Broadcast::Single(sender, vec![
                    GamePacket::serialize(
                        &TunneledPacket {
                            unknown1: true,
                            inner: MountReply {
                                rider_guid: sender,
                                mount_guid: 2,
                                unknown1: 0,
                                queue_pos: 1,
                                unknown3: 1,
                                composite_effect: 0,
                                unknown5: 0,
                            },
                        }
                    )?
                ])
            ]),
            _ => {
                println!("Unimplemented mount op code: {:?}", op_code);
                Ok(Vec::new())
            }
        },
        Err(_) => {
            println!("Unknown mount op code: {}", raw_op_code);
            Err(ProcessPacketError::CorruptedPacket)
        }
    }
}

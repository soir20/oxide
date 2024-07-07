use std::io::Cursor;

use crate::game_server::game_packet::{GamePacket, OpCode};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{tunnel::TunneledPacket, Broadcast, GameServer, ProcessPacketError};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum StoreOpCode {
    ItemList = 0x1,
    ItemDefinitionsReply = 0x3,
    ItemDefinitionsRequest = 0x8
}

impl SerializePacket for StoreOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Store.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub fn process_store_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match StoreOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            StoreOpCode::ItemDefinitionsRequest => {
                Ok(vec![Broadcast::Single(sender, vec![
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: StoreItemDefinitionsReply {
                            unknown: true,
                            defs: vec![],
                        }
                    })?
                ])])
            }
            _ => {
                println!("Unimplemented store packet: {:?}", op_code);
                Ok(Vec::new())
            }
        },
        Err(_) => {
            println!("Unknown store packet: {}", raw_op_code);
            Ok(Vec::new())
        }
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreItem {
    guid: u32,
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: bool,
    unknown5: bool,
    unknown6: u32,
    unknown7: bool,
    unknown8: bool,
    unknown9: u32,
    unknown10: u32,
    unknown11: u32,
    unknown12: u32,
    unknown13: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreItemList {
    pub static_items: Vec<StoreItem>,
    pub dynamic_items: Vec<StoreItem>,
}

impl GamePacket for StoreItemList {
    type Header = StoreOpCode;
    const HEADER: Self::Header = StoreOpCode::ItemList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreItemDefinitionsReply {
    pub unknown: bool,
    pub defs: Vec<u32>,
}

impl GamePacket for StoreItemDefinitionsReply {
    type Header = StoreOpCode;
    const HEADER: Self::Header = StoreOpCode::ItemDefinitionsReply;
}

use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum PurchaseOpCode {
    StoreCategories          = 0xe
}

impl SerializePacket for PurchaseOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Purchase.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreCategory {
    pub guid: u32,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreCategories {
    categories: Vec<StoreCategory>
}

impl GamePacket for StoreCategories {
    type Header = PurchaseOpCode;
    const HEADER: Self::Header = PurchaseOpCode::StoreCategories;
}

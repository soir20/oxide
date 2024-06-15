use crate::game_server::game_packet::{GamePacket, OpCode};
use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

#[derive(Copy, Clone, Debug)]
pub enum PurchaseOpCode {
    StoreCategories = 0xe,
    StoreCategoryGroups = 0x2a,
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
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreCategories {
    pub categories: Vec<StoreCategory>,
}

impl GamePacket for StoreCategories {
    type Header = PurchaseOpCode;
    const HEADER: Self::Header = PurchaseOpCode::StoreCategories;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreCategoryGroup {
    pub guid: u32,
    pub unknown1: u32,
    pub unknown2: Vec<u32>,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StoreCategoryGroups {
    pub groups: Vec<StoreCategoryGroup>,
}

impl GamePacket for StoreCategoryGroups {
    type Header = PurchaseOpCode;
    const HEADER: Self::Header = PurchaseOpCode::StoreCategoryGroups;
}

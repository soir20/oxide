use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum PurchaseOpCode {
    StoreCategories = 0xe,
    Billboards = 0x28,
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
pub struct BillboardPanel {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub members_only: bool,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub swf_name: String,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Billboard {
    pub unknown1: u32,
    pub unknown2: u32,
    pub panels: Vec<BillboardPanel>,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct BillboardsData {
    pub billboards: Vec<Billboard>,
}

pub struct Billboards {
    pub data: BillboardsData,
}

impl SerializePacket for Billboards {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();
        self.data.serialize(&mut inner_buffer)?;
        buffer.write_u32::<LittleEndian>(self.data.billboards.len() as u32)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32 - 4)?;
        buffer.write_all(&inner_buffer[4..])?;
        Ok(())
    }
}

impl GamePacket for Billboards {
    type Header = PurchaseOpCode;

    const HEADER: Self::Header = PurchaseOpCode::Billboards;
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

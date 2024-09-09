use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum StoreOpCode {
    ItemList = 0x1,
    ItemDefinitionsReply = 0x3,
}

impl SerializePacket for StoreOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Store.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub struct StoreItem {
    pub guid: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: bool,
    pub unknown5: bool,
    pub unknown6: u32,
    pub unknown7: bool,
    pub unknown8: bool,
    pub base_cost: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub member_cost: u32,
}

impl SerializePacket for StoreItem {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.unknown2)?;
        buffer.write_u32::<LittleEndian>(self.unknown3)?;
        buffer.write_u8(self.unknown4 as u8)?;
        buffer.write_u8(self.unknown5 as u8)?;
        buffer.write_u32::<LittleEndian>(self.unknown6)?;
        buffer.write_u8(self.unknown7 as u8)?;
        buffer.write_u8(self.unknown8 as u8)?;
        buffer.write_u32::<LittleEndian>(self.base_cost)?;
        buffer.write_u32::<LittleEndian>(self.unknown10)?;
        buffer.write_u32::<LittleEndian>(self.unknown11)?;
        buffer.write_u32::<LittleEndian>(self.unknown12)?;
        buffer.write_u32::<LittleEndian>(self.member_cost)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct StoreItemList {
    pub static_items: Vec<StoreItem>,
    pub dynamic_items: Vec<StoreItem>,
}

impl GamePacket for StoreItemList {
    type Header = StoreOpCode;
    const HEADER: Self::Header = StoreOpCode::ItemList;
}

#[derive(SerializePacket)]
pub struct StoreItemDefinitionsReply {
    pub unknown: bool,
    pub defs: Vec<u32>,
}

impl GamePacket for StoreItemDefinitionsReply {
    type Header = StoreOpCode;
    const HEADER: Self::Header = StoreOpCode::ItemDefinitionsReply;
}

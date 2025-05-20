use packet_serialize::SerializePacket;

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum StoreOpCode {
    ItemList = 0x1,
    ItemDefinitionsReply = 0x3,
}

impl SerializePacket for StoreOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::Store.serialize(buffer);
        (*self as u16).serialize(buffer);
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
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.guid.serialize(buffer);
        self.guid.serialize(buffer);
        self.unknown2.serialize(buffer);
        self.unknown3.serialize(buffer);
        self.unknown4.serialize(buffer);
        self.unknown5.serialize(buffer);
        self.unknown6.serialize(buffer);
        self.unknown7.serialize(buffer);
        self.unknown8.serialize(buffer);
        self.base_cost.serialize(buffer);
        self.unknown10.serialize(buffer);
        self.unknown11.serialize(buffer);
        self.unknown12.serialize(buffer);
        self.member_cost.serialize(buffer);
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

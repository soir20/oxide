use packet_serialize::{SerializePacket, Skip};

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
    pub guid: i32,
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
    pub defs: Vec<i32>,
}

impl GamePacket for StoreItemDefinitionsReply {
    type Header = StoreOpCode;
    const HEADER: Self::Header = StoreOpCode::ItemDefinitionsReply;
}

#[derive(Copy, Clone, Debug)]
pub enum PurchaseOpCode {
    Redirect = 0x5,
    Categories = 0xe,
}

impl SerializePacket for PurchaseOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::Purchase.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(Copy, Clone, Debug)]
pub enum RedirectOpCode {
    Definitions = 0x2,
}

impl SerializePacket for RedirectOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        PurchaseOpCode::Redirect.serialize(buffer);
        (*self as u32).serialize(buffer);
    }
}

#[derive(SerializePacket)]
pub struct RedirectItem {
    guid: i32,
    unused: Skip<8>,
}

#[derive(SerializePacket)]
pub struct RedirectBundle {
    unused: Skip<77>,
    items: Vec<RedirectItem>,
}

#[derive(SerializePacket)]
pub struct RedirectItemDefinitions {
    store: i32,
    unused: Skip<20>,
    bundles: Vec<RedirectBundle>,
}

impl GamePacket for RedirectItemDefinitions {
    type Header = RedirectOpCode;
    const HEADER: Self::Header = RedirectOpCode::Definitions;
}

impl RedirectItemDefinitions {
    pub fn new(guid: i32) -> Self {
        RedirectItemDefinitions {
            store: 2,
            unused: Skip,
            bundles: vec![RedirectBundle {
                unused: Skip,
                items: vec![RedirectItem { guid, unused: Skip }],
            }],
        }
    }
}

#[derive(SerializePacket)]
struct RedirectCategory {
    unused1: Skip<20>,
    unused2: i32,
}

#[derive(SerializePacket)]
pub struct RedirectCategories {
    categories: Vec<RedirectCategory>,
}

impl GamePacket for RedirectCategories {
    type Header = PurchaseOpCode;
    const HEADER: Self::Header = PurchaseOpCode::Categories;
}

impl RedirectCategories {
    pub fn new() -> Self {
        RedirectCategories {
            categories: vec![RedirectCategory {
                unused1: Skip,
                unused2: 1,
            }],
        }
    }
}

use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};
use serde::Deserialize;

use super::{item::WieldType, GamePacket, OpCode};

#[allow(clippy::enum_variant_names)]
#[derive(Copy, Clone, Debug)]
pub enum ReferenceDataOpCode {
    ItemClassDefinitions = 0x1,
    CategoryDefinitions = 0x2,
    ItemGroupDefinitions = 0x4,
}

impl SerializePacket for ReferenceDataOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::ReferenceData.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub struct ItemClassDefinition {
    pub guid: i32,
    pub name: u32,
    pub icon_set_id: u32,
    pub wield_type: WieldType,
    pub stat_id: u32,
    pub battle_class_name_id: u32,
}

impl SerializePacket for ItemClassDefinition {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i32::<LittleEndian>(self.guid)?;
        buffer.write_i32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.name)?;
        buffer.write_u32::<LittleEndian>(self.icon_set_id)?;
        self.wield_type.serialize(buffer)?;
        buffer.write_u32::<LittleEndian>(self.stat_id)?;
        buffer.write_u32::<LittleEndian>(self.battle_class_name_id)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct ItemClassDefinitions {
    pub definitions: Vec<ItemClassDefinition>,
}

impl GamePacket for ItemClassDefinitions {
    type Header = ReferenceDataOpCode;

    const HEADER: Self::Header = ReferenceDataOpCode::ItemClassDefinitions;
}

#[derive(Clone, Deserialize)]
pub struct CategoryDefinition {
    pub guid: i32,
    pub name: u32,
    pub icon_set_id: u32,
    pub sort_order: i32,
    pub visible: bool,
}

impl SerializePacket for CategoryDefinition {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i32::<LittleEndian>(self.guid)?;
        buffer.write_i32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.name)?;
        buffer.write_u32::<LittleEndian>(self.icon_set_id)?;
        buffer.write_i32::<LittleEndian>(self.sort_order)?;
        buffer.write_u8(self.visible as u8)?;
        Ok(())
    }
}

#[derive(Clone, Deserialize)]
pub struct CategoryRelation {
    pub parent_guid: i32,
    pub child_guid: i32,
}

impl SerializePacket for CategoryRelation {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i32::<LittleEndian>(self.parent_guid)?;
        buffer.write_i32::<LittleEndian>(self.parent_guid)?;
        buffer.write_i32::<LittleEndian>(self.child_guid)?;
        Ok(())
    }
}

#[derive(Clone, Deserialize, SerializePacket)]
pub struct CategoryDefinitions {
    pub definitions: Vec<CategoryDefinition>,
    pub relations: Vec<CategoryRelation>,
}

impl GamePacket for CategoryDefinitions {
    type Header = ReferenceDataOpCode;
    const HEADER: Self::Header = ReferenceDataOpCode::CategoryDefinitions;
}

pub struct ItemGroupItem {
    pub guid: u32,
    pub unknown: u32,
}

impl SerializePacket for ItemGroupItem {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.unknown)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct ItemGroupDefinition {
    pub guid: i32,
    pub unknown2: i32,
    pub name_id: u32,
    pub description_id: u32,
    pub sort_order: u32,
    pub icon_set_id: u32,
    pub category: u32,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: bool,
    pub unknown12: u32,
    pub unknown13: u32,
    pub unknown14: u32,
    pub unknown16: String,
    pub unknown17: bool,
    pub items: Vec<ItemGroupItem>,
}

#[derive(SerializePacket)]
pub struct ItemGroupDefinitionsData {
    pub definitions: Vec<ItemGroupDefinition>,
}

pub struct ItemGroupDefinitions {
    pub data: ItemGroupDefinitionsData,
}

impl SerializePacket for ItemGroupDefinitions {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();
        self.data.serialize(&mut inner_buffer)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;
        Ok(())
    }
}

impl GamePacket for ItemGroupDefinitions {
    type Header = ReferenceDataOpCode;
    const HEADER: Self::Header = ReferenceDataOpCode::ItemGroupDefinitions;
}

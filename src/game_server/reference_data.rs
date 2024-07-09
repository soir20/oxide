use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, StringId};
use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use serde::Deserialize;
use std::{
    fs::File,
    io::{Error, Write},
    path::Path,
};

#[derive(Copy, Clone, Debug)]
pub enum ReferenceDataOpCode {
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

#[derive(Clone, Deserialize)]
pub struct CategoryDefinition {
    pub guid: i32,
    pub name: StringId,
    pub icon_set_id: ImageId,
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

#[derive(SerializePacket, DeserializePacket)]
pub struct ItemGroupDefinition {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: bool,
    pub unknown12: u32,
    pub unknown13: u32,
    pub unknown14: u32,
    pub unknown16: String,
    pub unknown17: bool,
}

#[derive(SerializePacket, DeserializePacket)]
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

pub fn load_categories(config_dir: &Path) -> Result<CategoryDefinitions, Error> {
    let mut file = File::open(config_dir.join("item_categories.json"))?;
    Ok(serde_json::from_reader(&mut file)?)
}

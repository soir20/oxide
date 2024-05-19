use std::io::Write;
use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, StringId};

#[derive(Copy, Clone, Debug)]
pub enum ReferenceDataOpCode {
    CategoryDefinitions             = 0x2,
    ItemGroupDefinitions            = 0x4
}

impl SerializePacket for ReferenceDataOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::ReferenceData.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub struct CategoryDefinition {
    pub guid: u32,
    pub name: StringId,
    pub icon_set_id: ImageId,
    pub unknown1: u32,
    pub unknown2: bool
}

impl SerializePacket for CategoryDefinition {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.name)?;
        buffer.write_u32::<LittleEndian>(self.icon_set_id)?;
        buffer.write_u32::<LittleEndian>(self.unknown1)?;
        buffer.write_u8(self.unknown2 as u8)?;
        Ok(())
    }
}

pub struct CategoryRelation {
    pub parent_guid: u32,
    pub child_guid: u32
}

impl SerializePacket for CategoryRelation {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(self.parent_guid)?;
        buffer.write_u32::<LittleEndian>(self.parent_guid)?;
        buffer.write_u32::<LittleEndian>(self.child_guid)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct CategoryDefinitions {
    pub definitions: Vec<CategoryDefinition>,
    pub relations: Vec<CategoryRelation>
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
    pub unknown17: bool
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ItemGroupDefinitionsData {
    pub definitions: Vec<ItemGroupDefinition>
}

pub struct ItemGroupDefinitions {
    pub data: ItemGroupDefinitionsData
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

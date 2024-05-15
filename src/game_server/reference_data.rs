use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, StringId};

#[derive(Copy, Clone, Debug)]
pub enum ReferenceDataOpCode {
    CategoryDefinitions             = 0x2,
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
    pub icon_id: ImageId,
    pub unknown1: u32,
    pub unknown2: bool
}

impl SerializePacket for CategoryDefinition {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.guid)?;
        buffer.write_u32::<LittleEndian>(self.name)?;
        buffer.write_u32::<LittleEndian>(self.icon_id)?;
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

use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{item::EquipmentSlot, OpCode};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum InventoryOpCode {
    UnequipSlot = 0x2,
    EquipGuid = 0x3,
    EquipSaber = 0xd,
}

impl SerializePacket for InventoryOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Inventory.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnequipSlot {
    pub slot: EquipmentSlot,
    pub battle_class: u32,
}

#[derive(DeserializePacket)]
pub struct EquipGuid {
    pub item_guid: u32,
    pub battle_class: u32,
    pub slot: EquipmentSlot,
}

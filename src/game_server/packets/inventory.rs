use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket};

use super::{item::EquipmentSlot, OpCode};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum InventoryOpCode {
    UnequipSlot = 0x2,
    EquipGuid = 0x3,
    PreviewCustomization = 0xb,
    EquipCustomization = 0xc,
    EquipSaber = 0xd,
}

impl SerializePacket for InventoryOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::Inventory.serialize(buffer);
        (*self as u16).serialize(buffer);
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

#[derive(DeserializePacket)]
pub struct PreviewCustomization {
    pub item_guid: u32,
}

#[derive(DeserializePacket)]
pub struct EquipCustomization {
    pub item_guid: u32,
}

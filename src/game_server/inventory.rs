use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};

use super::game_packet::OpCode;

#[derive(Copy, Clone, Debug)]
pub enum InventoryOpCode {
    Unequip = 0x2,
}

impl SerializePacket for InventoryOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Inventory.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum PurchaseOpCode {
    EnableMarketplace = 0x1c,
}

impl SerializePacket for PurchaseOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::Purchase.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct EnableMarketplace {
    pub enabled: bool,
}

impl GamePacket for EnableMarketplace {
    type Header = PurchaseOpCode;
    const HEADER: Self::Header = PurchaseOpCode::EnableMarketplace;
}

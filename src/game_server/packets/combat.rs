use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug)]
pub enum CombatOpCode {
    ProcessedAttack = 0x7,
}

impl SerializePacket for CombatOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::Combat.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ProcessedAttack {
    unknown1: u64,
    unknown2: u64,
    unknown3: u64,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: bool,
    unknown8: bool,
    unknown9: u32,
    unknown10: u32,
}

impl GamePacket for ProcessedAttack {
    type Header = CombatOpCode;
    const HEADER: Self::Header = CombatOpCode::ProcessedAttack;
}

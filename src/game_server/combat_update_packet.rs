use crate::game_server::game_packet::{GamePacket, OpCode};
use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

#[derive(Copy, Clone, Debug)]
pub enum CombatUpdateOpCode {
    ProcessedAttack = 0x7,
}

impl SerializePacket for CombatUpdateOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Combat.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
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
    type Header = CombatUpdateOpCode;
    const HEADER: Self::Header = CombatUpdateOpCode::ProcessedAttack;
}

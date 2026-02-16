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
    pub attacker_guid1: u64,
    pub attacker_guid2: u64,
    pub receiver_guid: u64,
    pub damage_dealt: u32,
    pub max_hp: u32,
    pub receiver_composite_effect: u32,
    pub use_hurt_animation: bool,
    pub unknown1: bool,
    pub attacker_composite_effect: u32,
    pub current_health: u32,
}

impl GamePacket for ProcessedAttack {
    type Header = CombatOpCode;
    const HEADER: Self::Header = CombatOpCode::ProcessedAttack;
}

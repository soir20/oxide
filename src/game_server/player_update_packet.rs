use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{Effect, GamePacket, OpCode};
use crate::game_server::player_data::Pos;

#[derive(Copy, Clone, Debug)]
pub enum PlayerUpdateOpCode {
    AddNpc                   = 0x2
}

impl SerializePacket for PlayerUpdateOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::PlayerUpdate.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub struct Attachment {
    unknown1: String,
    unknown2: String,
    unknown3: String,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
}

pub struct Unknown {
    unknown1: u32,
    unknown2: String,
    unknown3: String,
    unknown4: u32,
    unknown5: String,
}

pub struct Variable {
    unknown1: u32,
    unknown2: String,
    unknown3: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AddNpc {
    guid: u64,
    name_id: u32,
    model_id: u32,
    unknown3: bool,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: f32,
    position: Pos,
    rotation: Pos,
    unknown8: u32,
    attachments: Vec<Attachment>,
    unknown9: u32,
    unknown10: u32,
    texture_name: String,
    tint_name: String,
    tint_id: u32,
    unknown11: bool,
    unknown12: u32,
    unknown13: u32,
    unknown14: u32,
    name_override: String,
    hide_name: bool,
    unknown15: f32,
    unknown16: f32,
    unknown17: f32,
    unknown18: u32,
    unknown19: bool,
    unknown20: f32,
    unknown21: bool,
    unknown22: u32,
    unknown23: u32,
    unknown24: u32,
    unknown25: u32,
    unknown26: bool,
    unknown27: bool,
    unknown28: u32,
    unknown29: u32,
    unknown30: u32,
    unknown31: Vec<Effect>,
    unknown32: bool,
    unknown33: u32,
    unknown34: bool,
    unknown35: bool,
    unknown36: bool,
    unknown37: bool,
    unknown38: Unknown,
    unknown39: Pos,
    unknown40: u32,
    unknown41: u32,
    unknown42: u32,
    unknown43: bool,
    unknown44: u64,
    unknown45: u32,
    unknown46: f32,

    // TODO: fix target data types. Should sum to 12 bytes
    unknown47: u32,
    unknown48: u32,
    unknown49: u32,

    unknown50: Vec<Variable>,
    unknown51: u32,
    unknown52: f32,
    unknown53: Pos,
    unknown54: u32,
    unknown55: f32,
    unknown56: f32,
    unknown57: f32,
    unknown58: String,
    unknown59: String,
    unknown60: String,
    unknown61: bool,
    unknown62: u32,
    unknown63: u32,
    unknown64: u32,
    unknown65: u32,
    unknown66: u32,
    unknown67: u32,
    unknown68: bool,
    unknown69: f32,
    unknown70: f32,
    unknown71: u64,
    unknown72: u32,
}

impl GamePacket for AddNpc {
    type Header = PlayerUpdateOpCode;
    const HEADER: PlayerUpdateOpCode = PlayerUpdateOpCode::AddNpc;
}

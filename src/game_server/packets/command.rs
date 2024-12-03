use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode, Rgba, Target};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum CommandOpCode {
    InteractionList = 0x9,
    SelectPlayer = 0xf,
    ChatBubbleColor = 0xe,
    PlaySoundOnTarget = 0x22,
}

impl SerializePacket for CommandOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Command.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ChatBubbleColor {
    text_color: Rgba,
    bubble_color: Rgba,
    size: u32,
    guid: u64,
}

impl GamePacket for ChatBubbleColor {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::ChatBubbleColor;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Interaction {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InteractionList {
    pub guid: u64,
    pub unknown1: bool,
    pub interactions: Vec<Interaction>,
    pub unknown2: String,
    pub unknown3: bool,
    pub unknown4: bool,
}

impl GamePacket for InteractionList {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::InteractionList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SelectPlayer {
    pub requester: u64,
    pub target: u64,
}

impl GamePacket for SelectPlayer {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::SelectPlayer;
}

#[derive(SerializePacket)]
pub struct PlaySoundIdOnTarget {
    pub sound_id: u32,
    pub target: Target,
}

impl GamePacket for PlaySoundIdOnTarget {
    type Header = CommandOpCode;

    const HEADER: Self::Header = CommandOpCode::PlaySoundOnTarget;
}

use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::{Broadcast, GameServer, ProcessPacketError};
use crate::game_server::zone::interact_with_character;

pub fn process_command(game_server: &GameServer, cursor: &mut Cursor<&[u8]>) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match CommandOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            CommandOpCode::SelectPlayer => {
                let req = SelectPlayer::deserialize(cursor)?;
                interact_with_character(req, game_server)
            },
            _ => {
                println!("Unimplemented command: {:?}", op_code);
                Ok(Vec::new())
            }
        },
        Err(_) => {
            println!("Unknown command: {}", raw_op_code);
            Ok(Vec::new())
        }
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum CommandOpCode {
    InteractionList          = 0x9,
    SelectPlayer             = 0xf,
    ChatBubbleColor          = 0xe
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
    text_color: u32,
    bubble_color: u32,
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
    pub target: u64
}

impl GamePacket for SelectPlayer {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::SelectPlayer;
}

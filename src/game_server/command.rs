use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::{GameServer, ProcessPacketError};

pub fn process_command(game_server: &GameServer, cursor: &mut Cursor<&[u8]>) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match CommandOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            CommandOpCode::SelectPlayer => {
                let req = SelectPlayer::deserialize(cursor)?;

                let zones = game_server.read_zones();
                if let Some(zone_guid) = GameServer::zone_with_player(&zones, req.requester) {
                    Ok(zones.get(zone_guid).unwrap().read().interact_with_character(req)?)
                } else {
                    println!("Received interaction request from invalid requester {}", req.requester);
                    Err(ProcessPacketError::CorruptedPacket)
                }
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

#[derive(Copy, Clone, Debug)]
pub enum CommandOpCode {
    InteractionList          = 0x9,
    SelectPlayer = 0xf
}

impl SerializePacket for CommandOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Command.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub struct UnknownCommandOpCode;

impl TryFrom<u16> for CommandOpCode {
    type Error = UnknownCommandOpCode;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x9 => Ok(CommandOpCode::InteractionList),
            0xf => Ok(CommandOpCode::SelectPlayer),
            _ => Err(UnknownCommandOpCode)
        }
    }
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

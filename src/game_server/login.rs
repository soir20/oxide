use std::io::{Cursor, Error, Read, Write};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};
use crate::game_server::OpCode;
use crate::game_server::player_data::make_test_player;

pub fn make_tunneled_packet(op_code: u16, bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    buffer.write_u16::<LittleEndian>(5)?;
    buffer.write_u8(true as u8)?;
    buffer.write_u32::<LittleEndian>(bytes.len() as u32 + 2)?;
    buffer.write_u16::<LittleEndian>(op_code)?;
    buffer.write_all(&bytes)?;
    Ok(buffer)
}

pub fn extract_tunneled_packet_data(data: &[u8]) -> Result<(u16, Vec<u8>), Error> {
    let mut cursor = Cursor::new(data);
    let tunneled_op_code = cursor.read_u16::<LittleEndian>()?;
    if tunneled_op_code != 5 {
        // TODO: use custom error type
        panic!("Expected a tunneled packet, but found op code {}", tunneled_op_code);
    }

    cursor.read_u8()?;
    let size = cursor.read_u32::<LittleEndian>()?.checked_sub(2).unwrap_or(0);
    let op_code = cursor.read_u16::<LittleEndian>()?;
    let mut buffer = vec![0; size as usize];
    cursor.read_exact(&mut buffer)?;

    Ok((op_code, buffer))
}

pub fn send_item_definitions() -> Result<Vec<u8>, Error> {
    let mut bytes: Vec<u8> = vec![];
    let mut buffer = Vec::new();
    buffer.write_u16::<LittleEndian>(0x25)?;
    buffer.write_i32::<LittleEndian>(bytes.len() as i32)?;
    buffer.append(&mut bytes);
    make_tunneled_packet(0x23, &buffer)
}

pub fn send_player_data() -> Result<Vec<u8>, SerializePacketError> {
    let mut bytes = Vec::new();
    make_test_player().serialize(&mut bytes)?;
    let mut buffer = Vec::new();
    buffer.write_u32::<LittleEndian>(bytes.len() as u32)?;
    buffer.append(&mut bytes);
    let final_packet = make_tunneled_packet(OpCode::PlayerData as u16, &buffer)?;
    Ok(final_packet)
}
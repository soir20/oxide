use std::io::Cursor;
use std::mem::size_of;
use byteorder::{BigEndian, ReadBytesExt};
use crate::deserialize::DeserializeError;

//noinspection DuplicatedCode
fn read_data_bundle_variable_length_int(data: &[u8]) -> Result<(u32, usize), DeserializeError> {
    let mut cursor = Cursor::new(data);

    if data.len() >= 1 && data[0] < 0xFF {
        Ok((data[0] as u32, size_of::<u8>()))
    } else if data.len() >= 3 && data[1] == 0xFF && data[2] == 0xFF {
        cursor.set_position(3);
        Ok((cursor.read_u32::<BigEndian>()?, 3 + size_of::<u32>()))
    } else {
        cursor.set_position(1);
        Ok((cursor.read_u16::<BigEndian>()? as u32, 1 + size_of::<u16>()))
    }

}

fn unbundle_reliable_data(data: &[u8]) -> Result<Vec<Vec<u8>>, DeserializeError> {
    let mut offset = 0;
    let mut cursor = Cursor::new(data);
    let mut packets = Vec::new();

    while offset < data.len() {
        let (packet_length, new_offset) = read_data_bundle_variable_length_int(&data[offset..])?;
        offset += new_offset;
        cursor.set_position(offset as u64);

        offset += size_of::<u16>();
        let remaining_length = packet_length as usize - size_of::<u16>();
        packets.push(data[offset..(offset + remaining_length)].to_vec());
        offset += remaining_length;
    }

    Ok(packets)
}

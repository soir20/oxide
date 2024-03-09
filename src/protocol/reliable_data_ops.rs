use std::io::{Cursor, Error, Write};
use std::mem::size_of;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use crate::protocol::{BufferSize, Packet, ProtocolOpCode, Session};
use crate::protocol::serialize::max_fragment_data_size;

#[non_exhaustive]
#[derive(Debug)]
pub enum DataError {
    IoError(Error),
    MissingSession,
    MissingDataLength,
    ExpectedFragment(ProtocolOpCode),
}

impl From<Error> for DataError {
    fn from(value: Error) -> Self {
        DataError::IoError(value)
    }
}

pub enum DataPacket {
    Fragment(Vec<u8>),
    Single(Vec<u8>)
}

pub struct FragmentState {
    buffer: Vec<u8>,
    remaining_bytes: u32
}

impl FragmentState {
    pub fn new() -> Self {
        FragmentState { buffer: Vec::new(), remaining_bytes: 0 }
    }
    
    pub fn add(&mut self, packet: Packet) -> Result<Option<Packet>, DataError> {
        if let Packet::DataFragment(sequence_number, data) = packet {
            let packet_data;
            if self.remaining_bytes == 0 {
                if data.len() < 8 {
                    return Err(DataError::MissingDataLength);
                }

                packet_data = &data[4..];
                self.remaining_bytes = Cursor::new(&data).read_u32::<BigEndian>()?;
            } else {
                packet_data = &data;
            }

            self.remaining_bytes = self.remaining_bytes.checked_sub(packet_data.len() as u32)
                .unwrap_or(0);
            self.buffer.extend(packet_data);

            if self.remaining_bytes > 0 {
                return Ok(None);
            }

            let old_buffer = self.buffer.clone();
            self.buffer.clear();
            return Ok(Some(Packet::Data(sequence_number, old_buffer)))
        }

        if self.remaining_bytes > 0 {
            return Err(DataError::ExpectedFragment(packet.op_code()));
        }

        Ok(Some(packet))
    }
}

//noinspection DuplicatedCode
fn read_data_bundle_variable_length_int(data: &[u8]) -> Result<(u32, usize), DataError> {
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

pub fn unbundle_reliable_data(data: &[u8]) -> Result<Vec<Vec<u8>>, DataError> {

    // Check for the magic bytes 0x00, 0x19 that indicate data packets
    if data.len() < 2 || data[0] != 0x00 || data[1] != 0x19 {
        return Ok(vec![data.to_vec()]);
    }

    let mut offset = 2;
    let mut cursor = Cursor::new(data);
    cursor.set_position(offset as u64);
    let mut packets = Vec::new();

    // TODO: check packet length is valid
    while offset < data.len() {
        let (packet_length, new_offset) = read_data_bundle_variable_length_int(&data[offset..])?;
        offset += new_offset;
        cursor.set_position(offset as u64);

        packets.push(data[offset..(offset + packet_length as usize)].to_vec());
        offset += packet_length as usize;
    }

    Ok(packets)
}

pub fn fragment_data(buffer_size: BufferSize, possible_session: &Option<Session>,
                     data: Vec<u8>) -> Result<Vec<DataPacket>, DataError> {
    let mut remaining_data = &data[..];
    let mut is_first = true;
    let mut packets = Vec::new();

    if let Some(session) = possible_session {
        let max_size = max_fragment_data_size(buffer_size, session) as usize;

        if remaining_data.len() <= max_size {
            packets.push(DataPacket::Single(data));
            return Ok(packets);
        }

        while remaining_data.len() > 0 {
            let mut end = max_size.min(remaining_data.len());
            let mut buffer = Vec::new();
            if is_first {
                buffer.write_u32::<BigEndian>(data.len() as u32)?;
                end -= size_of::<u32>();
                is_first = false;
            }

            let fragment = &remaining_data[0..end];
            buffer.write_all(fragment)?;
            remaining_data = &remaining_data[end..];

            packets.push(DataPacket::Fragment(buffer));
        }

        Ok(packets)
    } else {
        Err(DataError::MissingSession)
    }
}

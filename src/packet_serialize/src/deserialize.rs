use std::io::{BufRead, Cursor, Error, Read};
use std::string::FromUtf8Error;
use byteorder::{LittleEndian, ReadBytesExt};
use crate::NullTerminatedString;

#[non_exhaustive]
#[derive(Debug)]
pub enum DeserializePacketError {
    IoError(Error),
    InvalidString(FromUtf8Error),
    MissingNullTerminator,
    UnknownDiscriminator
}

impl From<Error> for DeserializePacketError {
    fn from(value: Error) -> Self {
        DeserializePacketError::IoError(value)
    }
}

impl From<FromUtf8Error> for DeserializePacketError {
    fn from(value: FromUtf8Error) -> Self {
        DeserializePacketError::InvalidString(value)
    }
}

pub trait DeserializePacket {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError> where Self: Sized;
}

// Unsigned integers
impl DeserializePacket for u8 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u8, DeserializePacketError> {
        Ok(cursor.read_u8()?)
    }
}


impl DeserializePacket for u16 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u16, DeserializePacketError> {
        Ok(cursor.read_u16::<LittleEndian>()?)
    }
}

impl DeserializePacket for u32 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u32, DeserializePacketError> {
        Ok(cursor.read_u32::<LittleEndian>()?)
    }
}

impl DeserializePacket for u64 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u64, DeserializePacketError> {
        Ok(cursor.read_u64::<LittleEndian>()?)
    }
}

impl DeserializePacket for u128 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u128, DeserializePacketError> {
        Ok(cursor.read_u128::<LittleEndian>()?)
    }
}

// Signed integers
impl DeserializePacket for i8 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i8, DeserializePacketError> {
        Ok(cursor.read_i8()?)
    }
}


impl DeserializePacket for i16 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i16, DeserializePacketError> {
        Ok(cursor.read_i16::<LittleEndian>()?)
    }
}

impl DeserializePacket for i32 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i32, DeserializePacketError> {
        Ok(cursor.read_i32::<LittleEndian>()?)
    }
}

impl DeserializePacket for i64 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i64, DeserializePacketError> {
        Ok(cursor.read_i64::<LittleEndian>()?)
    }
}

impl DeserializePacket for i128 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i128, DeserializePacketError> {
        Ok(cursor.read_i128::<LittleEndian>()?)
    }
}

// Floats
impl DeserializePacket for f32 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<f32, DeserializePacketError> {
        Ok(cursor.read_f32::<LittleEndian>()?)
    }
}

impl DeserializePacket for f64 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<f64, DeserializePacketError> {
        Ok(cursor.read_f64::<LittleEndian>()?)
    }
}

// Other types
impl DeserializePacket for bool {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<bool, DeserializePacketError> {
        Ok(cursor.read_u8()? != 0)
    }
}

impl DeserializePacket for String {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<String, DeserializePacketError> {
        let length = cursor.read_u32::<LittleEndian>()?;
        let mut str_bytes = vec![0; length as usize];
        cursor.read_exact(&mut str_bytes)?;
        Ok(String::from_utf8(str_bytes)?)
    }
}

impl DeserializePacket for NullTerminatedString {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<NullTerminatedString, DeserializePacketError> {
        let mut str_bytes = Vec::new();
        cursor.read_until(0, &mut str_bytes)?;
        if let Some(last_byte) = str_bytes.pop() {
            if last_byte == 0 {
                return Ok(NullTerminatedString(String::from_utf8(str_bytes)?));
            }
        }

        Err(DeserializePacketError::MissingNullTerminator)
    }
}

impl<T: DeserializePacket> DeserializePacket for Vec<T> {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Vec<T>, DeserializePacketError> {
        let mut items = Vec::new();
        let length = cursor.read_u32::<LittleEndian>()?;

        for _ in 0..length {
            let item: T = DeserializePacket::deserialize(cursor)?;
            items.push(item);
        }

        Ok(items)
    }
}

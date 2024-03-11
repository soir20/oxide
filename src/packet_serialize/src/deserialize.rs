use std::io::{Cursor, Error};
use byteorder::{LittleEndian, ReadBytesExt};

#[non_exhaustive]
#[derive(Debug)]
pub enum PacketDeserializeError {
    IoError(Error)
}

impl From<Error> for PacketDeserializeError {
    fn from(value: Error) -> Self {
        PacketDeserializeError::IoError(value)
    }
}

pub trait PacketDeserialize {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, PacketDeserializeError> where Self: Sized;
}

// Unsigned integers
impl PacketDeserialize for u8 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u8, PacketDeserializeError> {
        Ok(cursor.read_u8()?)
    }
}


impl PacketDeserialize for u16 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u16, PacketDeserializeError> {
        Ok(cursor.read_u16::<LittleEndian>()?)
    }
}

impl PacketDeserialize for u32 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u32, PacketDeserializeError> {
        Ok(cursor.read_u32::<LittleEndian>()?)
    }
}

impl PacketDeserialize for u64 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u64, PacketDeserializeError> {
        Ok(cursor.read_u64::<LittleEndian>()?)
    }
}

impl PacketDeserialize for u128 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u128, PacketDeserializeError> {
        Ok(cursor.read_u128::<LittleEndian>()?)
    }
}

// Signed integers
impl PacketDeserialize for i8 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i8, PacketDeserializeError> {
        Ok(cursor.read_i8()?)
    }
}


impl PacketDeserialize for i16 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i16, PacketDeserializeError> {
        Ok(cursor.read_i16::<LittleEndian>()?)
    }
}

impl PacketDeserialize for i32 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i32, PacketDeserializeError> {
        Ok(cursor.read_i32::<LittleEndian>()?)
    }
}

impl PacketDeserialize for i64 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i64, PacketDeserializeError> {
        Ok(cursor.read_i64::<LittleEndian>()?)
    }
}

impl PacketDeserialize for i128 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<i128, PacketDeserializeError> {
        Ok(cursor.read_i128::<LittleEndian>()?)
    }
}

// Floats
impl PacketDeserialize for f32 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<f32, PacketDeserializeError> {
        Ok(cursor.read_f32::<LittleEndian>()?)
    }
}

impl PacketDeserialize for f64 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<f64, PacketDeserializeError> {
        Ok(cursor.read_f64::<LittleEndian>()?)
    }
}

// Other types
impl PacketDeserialize for bool {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<bool, PacketDeserializeError> {
        Ok(cursor.read_u8()? != 0)
    }
}

impl<T: PacketDeserialize> PacketDeserialize for Vec<T> {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Vec<T>, PacketDeserializeError> {
        let mut items = Vec::new();
        let length = cursor.read_u32::<LittleEndian>()?;

        for _ in 0..length {
            let item: T = PacketDeserialize::deserialize(cursor)?;
            items.push(item);
        }

        Ok(items)
    }
}

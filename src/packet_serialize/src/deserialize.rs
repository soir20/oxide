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

impl PacketDeserialize for u16 {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<u16, PacketDeserializeError> {
        Ok(cursor.read_u16::<LittleEndian>()?)
    }
}

use std::io::{Error, Write};
use byteorder::{LittleEndian, WriteBytesExt};

#[non_exhaustive]
#[derive(Debug)]
pub enum PacketSerializeError {
    IoError(Error)
}

impl From<Error> for PacketSerializeError {
    fn from(value: Error) -> Self {
        PacketSerializeError::IoError(value)
    }
}

pub trait PacketSerialize {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError>;
}

// Unsigned integers
impl PacketSerialize for u8 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_u8(*self)?;
        Ok(())
    }
}


impl PacketSerialize for u16 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_u16::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl PacketSerialize for u32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_u32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl PacketSerialize for u64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_u64::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl PacketSerialize for u128 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_u128::<LittleEndian>(*self)?;
        Ok(())
    }
}

// Signed integers
impl PacketSerialize for i8 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_i8(*self)?;
        Ok(())
    }
}


impl PacketSerialize for i16 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_i16::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl PacketSerialize for i32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_i32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl PacketSerialize for i64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_i64::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl PacketSerialize for i128 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_i128::<LittleEndian>(*self)?;
        Ok(())
    }
}

// Floats
impl PacketSerialize for f32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_f32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl PacketSerialize for f64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_f64::<LittleEndian>(*self)?;
        Ok(())
    }
}

// Other types
impl PacketSerialize for bool {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_u8(*self as u8)?;
        Ok(())
    }
}

impl PacketSerialize for String {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        buffer.write_u32::<LittleEndian>(self.len() as u32)?;
        buffer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl<T: PacketSerialize> PacketSerialize for Vec<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), PacketSerializeError> {
        for index in 0..self.len() {
            PacketSerialize::serialize(&self[index], buffer)?;
        }

        Ok(())
    }
}

use std::io::{Error, Write};
use byteorder::{LittleEndian, WriteBytesExt};
use crate::LengthlessVec;

#[non_exhaustive]
#[derive(Debug)]
pub enum SerializePacketError {
    IoError(Error)
}

impl From<Error> for SerializePacketError {
    fn from(value: Error) -> Self {
        SerializePacketError::IoError(value)
    }
}

pub trait SerializePacket {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError>;
}

// Unsigned integers
impl SerializePacket for u8 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u8(*self)?;
        Ok(())
    }
}


impl SerializePacket for u16 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u16::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl SerializePacket for u32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl SerializePacket for u64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u64::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl SerializePacket for u128 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u128::<LittleEndian>(*self)?;
        Ok(())
    }
}

// Signed integers
impl SerializePacket for i8 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i8(*self)?;
        Ok(())
    }
}


impl SerializePacket for i16 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i16::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl SerializePacket for i32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl SerializePacket for i64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i64::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl SerializePacket for i128 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i128::<LittleEndian>(*self)?;
        Ok(())
    }
}

// Floats
impl SerializePacket for f32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_f32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl SerializePacket for f64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_f64::<LittleEndian>(*self)?;
        Ok(())
    }
}

// Other types
impl SerializePacket for bool {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u8(*self as u8)?;
        Ok(())
    }
}

impl SerializePacket for String {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(self.len() as u32)?;
        buffer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl<T: SerializePacket> SerializePacket for Vec<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        SerializePacket::serialize(&(self.len() as u32), buffer)?;
        for index in 0..self.len() {
            SerializePacket::serialize(&self[index], buffer)?;
        }

        Ok(())
    }
}

impl<T: SerializePacket> SerializePacket for LengthlessVec<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let inner_vec = &self.0;
        for index in 0..inner_vec.len() {
            SerializePacket::serialize(&inner_vec[index], buffer)?;
        }

        Ok(())
    }
}

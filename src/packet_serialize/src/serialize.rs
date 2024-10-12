use crate::{LengthlessVec, NullTerminatedString};
use byteorder::{LittleEndian, WriteBytesExt};
use std::{
    backtrace::Backtrace,
    collections::BTreeMap,
    io::{Error, Write},
};

#[non_exhaustive]
#[derive(Debug)]
pub enum SerializePacketError {
    IoError(Error, Backtrace),
}

impl From<Error> for SerializePacketError {
    fn from(value: Error) -> Self {
        SerializePacketError::IoError(value, Backtrace::force_capture())
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

impl SerializePacket for NullTerminatedString {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_all(self.0.as_bytes())?;
        buffer.write_u8(0)?;
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

impl<K, V: SerializePacket> SerializePacket for BTreeMap<K, V> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        SerializePacket::serialize(&(self.len() as u32), buffer)?;
        for value in self.values() {
            SerializePacket::serialize(value, buffer)?;
        }

        Ok(())
    }
}

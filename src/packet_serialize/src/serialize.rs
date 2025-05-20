use crate::{LengthlessSlice, LengthlessVec, NullTerminatedString};
use byteorder::{LittleEndian, WriteBytesExt};
use serde::de::IgnoredAny;
use std::{collections::BTreeMap, io::Write};

pub trait SerializePacket {
    fn serialize(&self, buffer: &mut Vec<u8>);
}

// Unsigned integers
impl SerializePacket for u8 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.write_u8(*self).expect("Unable to write u8");
    }
}

impl SerializePacket for u16 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_u16::<LittleEndian>(*self)
            .expect("Unable to write u16");
    }
}

impl SerializePacket for u32 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_u32::<LittleEndian>(*self)
            .expect("Unable to write u32");
    }
}

impl SerializePacket for u64 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_u64::<LittleEndian>(*self)
            .expect("Unable to write u64");
    }
}

impl SerializePacket for u128 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_u128::<LittleEndian>(*self)
            .expect("Unable to write u128");
    }
}

// Signed integers
impl SerializePacket for i8 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.write_i8(*self).expect("Unable to write i8");
    }
}

impl SerializePacket for i16 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_i16::<LittleEndian>(*self)
            .expect("Unable to write i16");
    }
}

impl SerializePacket for i32 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_i32::<LittleEndian>(*self)
            .expect("Unable to write i32");
    }
}

impl SerializePacket for i64 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_i64::<LittleEndian>(*self)
            .expect("Unable to write i64");
    }
}

impl SerializePacket for i128 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_i128::<LittleEndian>(*self)
            .expect("Unable to write i128");
    }
}

// Floats
impl SerializePacket for f32 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_f32::<LittleEndian>(*self)
            .expect("Unable to write f32");
    }
}

impl SerializePacket for f64 {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_f64::<LittleEndian>(*self)
            .expect("Unable to write f64");
    }
}

// Other types
impl SerializePacket for bool {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.write_u8(*self as u8).expect("Unable to write bool");
    }
}

impl SerializePacket for String {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_u32::<LittleEndian>(self.len() as u32)
            .expect("Unable to write string length");
        buffer
            .write_all(self.as_bytes())
            .expect("Unable to write string");
    }
}

impl SerializePacket for NullTerminatedString {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer
            .write_all(self.0.as_bytes())
            .expect("Unable to write null-terminated string");
        buffer
            .write_u8(0)
            .expect("Unable to write string null terminator");
    }
}

impl<T: SerializePacket> SerializePacket for &[T] {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        SerializePacket::serialize(&(self.len() as u32), buffer);
        for index in 0..self.len() {
            SerializePacket::serialize(&self[index], buffer);
        }
    }
}

impl<T: SerializePacket> SerializePacket for Vec<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.as_slice().serialize(buffer);
    }
}

impl<'a, T: SerializePacket> SerializePacket for LengthlessSlice<'a, T> {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let inner_slice = self.0;
        for index in 0..inner_slice.len() {
            SerializePacket::serialize(&inner_slice[index], buffer);
        }
    }
}

impl<T: SerializePacket> SerializePacket for LengthlessVec<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        LengthlessSlice(self.0.as_slice()).serialize(buffer);
    }
}

impl<K, V: SerializePacket> SerializePacket for BTreeMap<K, V> {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        SerializePacket::serialize(&(self.len() as u32), buffer);
        for value in self.values() {
            SerializePacket::serialize(value, buffer);
        }
    }
}

impl SerializePacket for IgnoredAny {
    fn serialize(&self, _: &mut Vec<u8>) {}
}

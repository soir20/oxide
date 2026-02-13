mod deserialize;
mod serialize;

pub use deserialize::*;
pub use packet_serialize_derive::{DeserializePacket, SerializePacket};
pub use serialize::*;

#[derive(Clone)]
pub struct LengthlessVec<T>(pub Vec<T>);

#[derive(Clone)]
pub struct LengthlessSlice<'a, T>(pub &'a [T]);

#[derive(Clone)]
pub struct NullTerminatedString(pub String);

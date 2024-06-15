mod deserialize;
mod serialize;

pub use deserialize::*;
pub use packet_serialize_derive::{DeserializePacket, SerializePacket};
pub use serialize::*;

pub struct LengthlessVec<T>(pub Vec<T>);

pub struct NullTerminatedString(pub String);

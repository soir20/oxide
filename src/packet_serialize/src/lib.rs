mod deserialize;
mod serialize;

pub use deserialize::*;
pub use serialize::*;
pub use packet_serialize_derive::{SerializePacket, DeserializePacket};

pub struct LengthlessVec<T>(pub Vec<T>);

pub struct NullTerminatedString(pub String);

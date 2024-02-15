use crate::protocol::{BufferSize, Packet, Session};
#[non_exhaustive]
pub enum SerializeError {

}

pub fn serialize_packets(packets: &[Packet], buffer_size: BufferSize,
                         possible_session: &Option<Session>) -> Result<Vec<Vec<u8>>, SerializeError> {
    Ok(Vec::new())
}

use std::io::{Cursor, Read};

use packet_serialize::{DeserializePacket, DeserializePacketError};

/// Client -> server (op 0xbd) when a UI button is clicked.  The first byte is
/// a sub-type: 4 = this window/button/param form.
#[derive(Debug)]
pub struct UiInteraction {
    pub unknown1: u8,
    pub window_name: String,
    pub button_name: String,
    pub param: Option<String>,
    pub trailing: Vec<u8>,
}

impl DeserializePacket for UiInteraction {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError> {
        let unknown1: u8 = DeserializePacket::deserialize(cursor)?;
        let window_name: String = DeserializePacket::deserialize(cursor)?;
        let button_name: String = DeserializePacket::deserialize(cursor)?;

        let remaining = cursor.get_ref().len() as u64 - cursor.position();
        let param = if remaining > 0 {
            Some(String::deserialize(cursor)?)
        } else {
            None
        };

        let mut trailing = Vec::new();
        cursor.read_to_end(&mut trailing).ok();

        Ok(UiInteraction {
            unknown1,
            window_name,
            button_name,
            param,
            trailing,
        })
    }
}

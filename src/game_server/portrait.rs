use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};

use super::{
    game_packet::{GamePacket, OpCode},
    player_update_packet::Attachment,
};

#[derive(Copy, Clone, Debug)]
pub enum PortraitOpCode {
    ImageData = 0x4,
}

impl SerializePacket for PortraitOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Portrait.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct ImageData {
    pub guid: u64,
    pub image_name: String,
    pub unknown2: bool,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u64,
    pub unknown6: u32,
    pub attachments: Vec<Attachment>,
    pub head_model: String,
    pub hair_model: String,
    pub hair_color: u32,
    pub eye_color: u32,
    pub skin_tone: String,
    pub face_paint: String,
    pub facial_hair: String,
    pub unknown14: u32,
    pub unknown15: f32,
    pub unknown16: u32,
    pub unknown17: u32,
    pub png_name: String,
    pub unknown18: u32,
    pub png_data: Vec<u8>,
}

impl GamePacket for ImageData {
    type Header = PortraitOpCode;

    const HEADER: Self::Header = PortraitOpCode::ImageData;
}

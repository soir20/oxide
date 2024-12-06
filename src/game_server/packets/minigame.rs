use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MinigameOpCode {
    AllMinigameData = 0x1,
    MinigameGroup = 0x33,
    ShowStageSelect = 0x34,
}

impl SerializePacket for MinigameOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Minigame.serialize(buffer)?;
        buffer.write_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameHeader {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameData {
    pub stage_id: u32,
    pub stage_type: u32,
    pub stage_name_id: u32,
    pub stage_description_id: u32,
    pub stage_icon_set_id: u32,
    pub stage_difficulty: u32,
    pub members_only: bool,
    pub unknown8: u32,
    pub unknown9: String,
    pub unknown10: u32,
    pub stage_icon_id: u32,
    pub start_sound_id: u32,
    pub unknown13: String,
    pub unknown14: u32,
    pub unknown15: u32,
    pub unknown16: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameGroupLink {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: String,
    pub unknown7: String,
    pub unknown8: u32,
    pub unknown9: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameGroupData {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: String,
    pub unknown7: String,
    pub unknown8: bool,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub unknown13: u32,
    pub group_links: Vec<MinigameGroupLink>,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameTypeData {
    pub type_id: u32,
    pub type_name: u32,
    pub type_description: u32,
    pub members_only: bool,
    pub is_flash: bool, // unconfirmed
    pub is_micro: bool,
    pub is_active: bool,
    pub param1: u32, // unconfirmed
    pub icon_id: u32,
    pub background_icon_id: u32,
    pub is_popular: bool,
    pub is_game_of_day: bool,
    pub category_id: u32,
    pub display_order: u32,
    pub tutorial_swf: String,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownMinigameArray {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

pub struct AllMinigameData {
    pub header: MinigameHeader,
    pub minigame_data: Vec<MinigameData>,
    pub minigame_group_data: Vec<MinigameGroupData>,
    pub minigame_type_data: Vec<MinigameTypeData>,
    pub unknown: Vec<UnknownMinigameArray>,
}

impl SerializePacket for AllMinigameData {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();
        SerializePacket::serialize(&self.minigame_data, &mut inner_buffer)?;
        SerializePacket::serialize(&self.minigame_group_data, &mut inner_buffer)?;
        SerializePacket::serialize(&self.minigame_type_data, &mut inner_buffer)?;
        SerializePacket::serialize(&self.unknown, &mut inner_buffer)?;

        SerializePacket::serialize(&self.header, buffer)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;
        Ok(())
    }
}

impl GamePacket for AllMinigameData {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AllMinigameData;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Minigame {
    pub minigame_id: u32,
    pub minigame_type: u32,
    pub link_name: String,
    pub short_name: String,
    pub unlocked: bool,
    pub unknown6: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub parent_minigame_id: u32,
    pub members_only: bool,
    pub unknown12: u32,
    pub background_swf: String,
    pub min_players: u32,
    pub max_players: u32,
    pub stage_number: u32,
    pub required_item_id: u32,
    pub unknown18: u32,
    pub completed: bool,
    pub link_group_id: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct GroupInfo {
    pub header: MinigameHeader,
    pub group_id: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_id: u32,
    pub background_swf: String,
    pub default_game_id: u32,
    pub minigames: Vec<Minigame>,
    pub stage_progression: String,
    pub show_start_screen_on_play_next: bool,
    pub settings_icon_id: u32,
}

impl GamePacket for GroupInfo {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::MinigameGroup;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ShowStageSelect {
    pub header: MinigameHeader,
}

impl GamePacket for ShowStageSelect {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::ShowStageSelect;
}

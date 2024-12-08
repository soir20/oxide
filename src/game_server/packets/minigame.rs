use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MinigameOpCode {
    MinigameDefinitions = 0x1,
    CreateMinigameInstance = 0x11,
    StartGame = 0x12,
    CreateMinigameStageGroupInstance = 0x33,
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
pub struct RewardBundle {
    pub unknown1: bool,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub unknown13: u32,
    pub unknown14: u32,
    pub unknown15: u32,
    pub unknown16: Vec<u32>,
    pub unknown17: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameRewardBundle {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: bool,
    pub reward_bundle: RewardBundle,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: bool,
    pub unknown11: u32,
    pub unknown12: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct CreateMinigameInstance {
    pub header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: bool,
    pub unknown8: bool,
    pub reward_bundle1: RewardBundle,
    pub reward_bundle2: RewardBundle,
    pub reward_bundle3: RewardBundle,
    pub reward_bundles: Vec<MinigameRewardBundle>,
    pub unknown13: bool,
    pub unknown14: bool,
    pub unknown15: bool,
    pub unknown16: bool,
    pub unknown17: bool,
    pub unknown18: String,
    pub unknown19: u32,
    pub unknown20: bool,
    pub unknown21: u32,
    pub unknown22: bool,
    pub unknown23: bool,
    pub unknown24: bool,
    pub unknown25: u32,
    pub unknown26: u32,
    pub unknown27: u32,
}

impl GamePacket for CreateMinigameInstance {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::CreateMinigameInstance;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameStageDefinition {
    pub guid: u32,
    pub portal_entry_guid: u32,
    pub start_screen_name_id: u32,
    pub start_screen_description_id: u32,
    pub start_screen_icon_set_id: u32,
    pub difficulty: u32,
    pub members_only: bool,
    pub unknown8: u32,
    pub unknown9: String,
    pub unknown10: u32,
    pub unknown11: u32,
    pub start_sound_id: u32,
    pub unknown13: String,
    pub unknown14: u32,
    pub unknown15: u32,
    pub unknown16: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameStageGroupLink {
    pub link_id: u32,
    pub stage_group_definition_guid: u32,
    pub parent_game_id: u32,
    pub link_stage_definition_guid: u32,
    pub unknown5: u32,
    pub unknown6: String,
    pub unknown7: String,
    pub unknown8: u32,
    pub unknown9: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameStageGroupDefinition {
    pub guid: u32,
    pub portal_entry_guid: u32, // unconfirmed
    pub name_id: u32,
    pub description_id: u32, // unconfirmed
    pub icon_set_id: u32,
    pub stage_select_map_name: String,        // unconfirmed
    pub stage_progression: String,            // unconfirmed
    pub show_start_screen_on_play_next: bool, // unconfirmed
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub unknown13: u32,
    pub group_links: Vec<MinigameStageGroupLink>,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigamePortalEntry {
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub members_only: bool,
    pub is_flash: bool, // unconfirmed
    pub is_micro: bool,
    pub is_active: bool,
    pub param1: u32, // unconfirmed
    pub icon_id: u32,
    pub background_icon_id: u32,
    pub is_popular: bool,
    pub is_game_of_day: bool,
    pub portal_category_guid: u32,
    pub sort_order: u32,
    pub tutorial_swf: String,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigamePortalCategory {
    pub guid: u32,
    pub name_id: u32,
    pub icon_set_id: u32,
    pub sort_order: u32,
}

pub struct MinigameDefinitions {
    pub header: MinigameHeader,
    pub stages: Vec<MinigameStageDefinition>,
    pub stage_groups: Vec<MinigameStageGroupDefinition>,
    pub portal_entries: Vec<MinigamePortalEntry>,
    pub portal_categories: Vec<MinigamePortalCategory>,
}

impl SerializePacket for MinigameDefinitions {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();
        SerializePacket::serialize(&self.stages, &mut inner_buffer)?;
        SerializePacket::serialize(&self.stage_groups, &mut inner_buffer)?;
        SerializePacket::serialize(&self.portal_entries, &mut inner_buffer)?;
        SerializePacket::serialize(&self.portal_categories, &mut inner_buffer)?;

        SerializePacket::serialize(&self.header, buffer)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;
        Ok(())
    }
}

impl GamePacket for MinigameDefinitions {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::MinigameDefinitions;
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
pub struct CreateMinigameStageGroupInstance {
    pub header: MinigameHeader,
    pub group_id: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_id: u32,
    pub stage_select_map_name: String,
    pub default_game_id: u32,
    pub minigames: Vec<Minigame>,
    pub stage_progression: String,
    pub show_start_screen_on_play_next: bool,
    pub settings_icon_id: u32,
}

impl GamePacket for CreateMinigameStageGroupInstance {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::CreateMinigameStageGroupInstance;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ShowStageSelect {
    pub header: MinigameHeader,
}

impl GamePacket for ShowStageSelect {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::ShowStageSelect;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StartGame {
    pub header: MinigameHeader,
}

impl GamePacket for StartGame {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::StartGame;
}

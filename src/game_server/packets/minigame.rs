use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode, RewardBundle};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MinigameOpCode {
    MinigameDefinitions = 0x1,
    RequestCreateActiveMinigame = 0x4,
    RequestStartActiveMinigame = 0x6,
    RequestCancelActiveMinigame = 0x7,
    FlashPayload = 0xf,
    CreateActiveMinigame = 0x11,
    StartActiveMinigame = 0x12,
    EndActiveMinigame = 0x13,
    LeaveActiveMinigame = 0x14,
    UpdateActiveMinigameRewards = 0x17,
    ActiveMinigameEndScore = 0x30,
    RequestMinigameStageGroupInstance = 0x31,
    CreateMinigameStageGroupInstance = 0x33,
    ShowStageInstanceSelect = 0x34,
    ActiveMinigameCreationResult = 0x44,
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
    pub stage_guid: i32,
    pub unknown2: i32,
    pub stage_group_guid: i32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameStageDefinition {
    pub guid: i32,
    pub portal_entry_guid: u32,
    pub start_screen_name_id: u32,
    pub start_screen_description_id: u32,
    pub start_screen_icon_id: u32,
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
    pub parent_stage_group_definition_guid: i32,
    pub parent_stage_definition_guid: i32,
    pub child_stage_definition_guid: i32,
    pub icon_id: u32,
    pub link_name: String,
    pub short_name: String,
    pub stage_number: u32,
    pub child_stage_group_definition_guid: i32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameStageGroupDefinition {
    pub guid: i32,
    pub portal_entry_guid: u32, // unconfirmed
    pub name_id: u32,
    pub description_id: u32, // unconfirmed
    pub icon_id: u32,
    pub stage_select_map_name: String,        // unconfirmed
    pub stage_progression: String,            // unconfirmed
    pub show_start_screen_on_play_next: bool, // unconfirmed
    pub settings_icon_id: u32,
    pub opened_from_portal_entry_guid: u32,
    pub required_item_id: u32,
    pub required_bundle_id: u32,
    pub required_prereq_item_id: u32,
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
    pub icon_id: u32,
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
pub struct RequestCreateActiveMinigame {
    pub header: MinigameHeader,
}

impl GamePacket for RequestCreateActiveMinigame {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::RequestCreateActiveMinigame;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RequestCancelActiveMinigame {
    pub header: MinigameHeader,
}

impl GamePacket for RequestCancelActiveMinigame {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::RequestCancelActiveMinigame;
}

pub struct FlashPayload {
    pub header: MinigameHeader,
    pub payload: String,
}

impl SerializePacket for FlashPayload {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        self.header.serialize(buffer)?;
        buffer.write_u32::<LittleEndian>(self.payload.len().saturating_add(1) as u32)?;
        buffer.write_all(self.payload.as_bytes())?;
        buffer.write_u8(0)?;
        Ok(())
    }
}

impl DeserializePacket for FlashPayload {
    fn deserialize(
        cursor: &mut std::io::Cursor<&[u8]>,
    ) -> Result<Self, packet_serialize::DeserializePacketError>
    where
        Self: Sized,
    {
        let header = MinigameHeader::deserialize(cursor)?;
        let payload = String::deserialize(cursor)?
            .trim_end_matches('\0')
            .to_string();
        Ok(FlashPayload { header, payload })
    }
}

impl GamePacket for FlashPayload {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::FlashPayload;
}

#[derive(SerializePacket)]
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

#[derive(SerializePacket)]
pub struct CreateActiveMinigame {
    pub header: MinigameHeader,
    pub name_id: u32,
    pub icon_set_id: u32,
    pub description_id: u32,
    pub difficulty: u32,
    pub battle_class_type: u32,
    pub portal_entry_guid: u32,
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
    pub show_end_score_screen: bool,
    pub unknown18: String,
    pub unknown19: u32,
    pub unknown20: bool,
    pub stage_definition_guid: i32,
    pub unknown22: bool,
    pub unknown23: bool,
    pub unknown24: bool,
    pub unknown25: u32,
    pub unknown26: u32,
    pub unknown27: u32,
}

impl GamePacket for CreateActiveMinigame {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::CreateActiveMinigame;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StartActiveMinigame {
    pub header: MinigameHeader,
}

impl GamePacket for StartActiveMinigame {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::StartActiveMinigame;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct EndActiveMinigame {
    pub header: MinigameHeader,
    pub won: bool,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

impl GamePacket for EndActiveMinigame {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::EndActiveMinigame;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct LeaveActiveMinigame {
    pub header: MinigameHeader,
}

impl GamePacket for LeaveActiveMinigame {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::LeaveActiveMinigame;
}

#[derive(SerializePacket)]
pub struct EarnedTrophy {
    pub unknown1: u32,
    pub unknown2: u32,
}

#[derive(SerializePacket)]
pub struct UpdateActiveMinigameRewards {
    pub header: MinigameHeader,
    pub reward_bundle1: RewardBundle,
    pub unknown1: u32,
    pub unknown2: u32,
    pub reward_bundle2: RewardBundle,
    pub earned_trophies: Vec<EarnedTrophy>,
}

impl GamePacket for UpdateActiveMinigameRewards {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::UpdateActiveMinigameRewards;
}

#[allow(dead_code)]
#[derive(Copy, Clone, Default)]
pub enum ScoreType {
    #[default]
    Counter = 0,
    Time = 2,
    Total = 4,
}

impl SerializePacket for ScoreType {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_i32::<LittleEndian>(*self as i32)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct ScoreEntry {
    pub entry_text: String,
    pub icon_set_id: u32,
    pub score_type: ScoreType,
    pub score_count: i32,
    pub score_max: i32,
    pub score_points: i32,
}

#[derive(SerializePacket)]
pub struct ActiveMinigameEndScore {
    pub header: MinigameHeader,
    pub scores: Vec<ScoreEntry>,
    pub unknown2: bool,
}

impl GamePacket for ActiveMinigameEndScore {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::ActiveMinigameEndScore;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MinigameStageInstance {
    pub stage_instance_guid: i32,
    pub portal_entry_guid: u32,
    pub link_name: String,
    pub short_name: String,
    pub unlocked: bool,
    pub unknown6: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_id: u32,
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
    pub stage_group_instance_guid: i32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RequestMinigameStageGroupInstance {
    pub header: MinigameHeader,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct CreateMinigameStageGroupInstance {
    pub header: MinigameHeader,
    pub stage_group_guid: i32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_id: u32,
    pub stage_select_map_name: String,
    pub default_stage_instance_guid: u32,
    pub stage_instances: Vec<MinigameStageInstance>,
    pub stage_progression: String,
    pub show_start_screen_on_play_next: bool,
    pub settings_icon_id: u32,
}

impl GamePacket for CreateMinigameStageGroupInstance {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::CreateMinigameStageGroupInstance;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ShowStageInstanceSelect {
    pub header: MinigameHeader,
}

impl GamePacket for ShowStageInstanceSelect {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::ShowStageInstanceSelect;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ActiveMinigameCreationResult {
    pub header: MinigameHeader,
    pub was_successful: bool,
}

impl GamePacket for ActiveMinigameCreationResult {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::ActiveMinigameCreationResult;
}

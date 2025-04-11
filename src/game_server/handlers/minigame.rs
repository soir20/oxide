use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Cursor, Error, ErrorKind, Read},
    iter,
    path::Path,
    sync::Arc,
    time::Instant,
};

use byteorder::ReadBytesExt;
use evalexpr::{context_map, eval_with_context, Value};
use num_enum::TryFromPrimitive;
use packet_serialize::DeserializePacket;
use serde::Deserialize;

use crate::{
    game_server::{
        handlers::character::MinigameStatus,
        packets::{
            chat::{ActionBarTextColor, SendStringId},
            client_update::UpdateCredits,
            command::StartFlashGame,
            item::EquipmentSlot,
            minigame::{
                ActiveMinigameCreationResult, ActiveMinigameEndScore, CreateActiveMinigame,
                CreateMinigameStageGroupInstance, EndActiveMinigame, FlashPayload,
                LeaveActiveMinigame, MinigameDefinitions, MinigameHeader, MinigameOpCode,
                MinigamePortalCategory, MinigamePortalEntry, MinigameStageDefinition,
                MinigameStageGroupDefinition, MinigameStageGroupLink, MinigameStageInstance,
                RequestCancelActiveMinigame, RequestCreateActiveMinigame,
                RequestMinigameStageGroupInstance, RequestStartActiveMinigame, ScoreEntry,
                ScoreType, ShowStageInstanceSelect, StartActiveMinigame,
                UpdateActiveMinigameRewards,
            },
            saber_strike::{SaberStrikeOpCode, SaberStrikeStageData},
            tunnel::TunneledPacket,
            ui::ExecuteScriptWithStringParams,
            GamePacket, RewardBundle,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    info, teleport_to_zone,
};

use super::{
    character::{CharacterMatchmakingGroupIndex, CharacterType, MatchmakingGroupStatus, Player},
    guid::GuidTableIndexer,
    item::SABER_ITEM_TYPE,
    lock_enforcer::{CharacterLockRequest, CharacterTableWriteHandle, ZoneTableWriteHandle},
    saber_strike::process_saber_strike_packet,
    unique_guid::{player_guid, shorten_player_guid},
};

#[derive(Clone)]
pub struct PlayerStageStats {
    pub completed: bool,
    pub high_score: i32,
}

#[derive(Clone, Default)]
pub struct PlayerMinigameStats {
    stage_guid_to_stats: BTreeMap<i32, PlayerStageStats>,
    trophy_stats: BTreeMap<i32, i32>,
}

impl PlayerMinigameStats {
    pub fn complete(&mut self, stage_guid: i32, score: i32) {
        self.stage_guid_to_stats
            .entry(stage_guid)
            .and_modify(|entry| {
                entry.completed = true;
                entry.high_score = score.max(entry.high_score);
            })
            .or_insert_with(|| PlayerStageStats {
                completed: true,
                high_score: score,
            });
    }

    pub fn has_completed(&self, stage_guid: i32) -> bool {
        self.stage_guid_to_stats
            .get(&stage_guid)
            .map(|stats| stats.completed)
            .unwrap_or(false)
    }

    pub fn update_trophy_progress(&mut self, trophy_guid: i32, delta: i32) {
        self.trophy_stats
            .entry(trophy_guid)
            .and_modify(|value| {
                *value = value.saturating_add(delta);
            })
            .or_insert(delta);
    }
}

#[derive(Clone, Copy, Deserialize)]
pub struct StageLocator {
    pub stage_group_guid: i32,
    pub stage_guid: i32,
}

#[derive(Clone, Deserialize)]
pub enum MinigameType {
    Flash { game_swf_name: String },
    SaberStrike { saber_strike_stage_id: u32 },
}

#[non_exhaustive]
#[derive(Clone, Default)]
pub enum MinigameTypeData {
    #[default]
    Empty,
    SaberStrike {
        obfuscated_score: i32,
    },
}

impl From<&MinigameType> for MinigameTypeData {
    fn from(value: &MinigameType) -> Self {
        match value {
            MinigameType::Flash { .. } => MinigameTypeData::default(),
            MinigameType::SaberStrike { .. } => MinigameTypeData::SaberStrike {
                obfuscated_score: 0,
            },
        }
    }
}

const CHALLENGE_LINK_NAME: &str = "challenge";
const GROUP_LINK_NAME: &str = "group";

#[derive(Deserialize)]
pub struct MinigameChallengeConfig {
    pub guid: i32,
    pub name_id: u32,
    pub description_id: u32,
    pub min_players: u32,
    pub max_players: u32,
    pub start_sound_id: u32,
    pub required_item_guid: Option<u32>,
    pub members_only: bool,
    pub minigame_type: MinigameType,
    pub zone_template_guid: u8,
    pub score_to_credits_expression: String,
    #[serde(default = "default_matchmaking_timeout_millis")]
    pub matchmaking_timeout_millis: u32,
    pub single_player_stage_guid: Option<StageLocator>,
}

impl MinigameChallengeConfig {
    pub fn has_completed(&self, player: &Player) -> bool {
        player.minigame_stats.has_completed(self.guid)
    }

    pub fn unlocked(&self, player: &Player, base_stage_completed: bool) -> bool {
        self.required_item_guid
            .map(|item_guid| player.inventory.contains(&item_guid))
            .unwrap_or(true)
            && base_stage_completed
            && (!self.members_only || player.member)
    }

    pub fn to_stage_definition(
        &self,
        portal_entry_guid: u32,
        base_stage: &MinigameCampaignStageConfig,
    ) -> MinigameStageDefinition {
        MinigameStageDefinition {
            guid: self.guid,
            portal_entry_guid,
            start_screen_name_id: self.name_id,
            start_screen_description_id: self.description_id,
            start_screen_icon_id: base_stage.start_screen_icon_id,
            difficulty: base_stage.difficulty,
            members_only: self.members_only,
            unknown8: 0,
            unknown9: "".to_string(),
            unknown10: 0,
            unknown11: 0,
            start_sound_id: self.start_sound_id,
            unknown13: "".to_string(),
            unknown14: 0,
            unknown15: 0,
            unknown16: 0,
        }
    }

    pub fn to_stage_instance(
        &self,
        portal_entry_guid: u32,
        player: &Player,
        base_stage: &MinigameStageInstance,
    ) -> MinigameStageInstance {
        MinigameStageInstance {
            stage_instance_guid: self.guid,
            portal_entry_guid,
            link_name: CHALLENGE_LINK_NAME.to_string(),
            short_name: "".to_string(),
            unlocked: self.unlocked(player, base_stage.completed),
            unknown6: 0,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_id: base_stage.icon_id,
            parent_stage_instance_guid: base_stage.stage_instance_guid,
            members_only: self.members_only,
            unknown12: 0,
            background_swf: "".to_string(),
            min_players: self.min_players,
            max_players: self.max_players,
            stage_number: 0,
            required_item_id: 0,
            unknown18: 0,
            completed: self.has_completed(player),
            stage_group_instance_guid: base_stage.stage_group_instance_guid,
        }
    }
}

#[derive(Deserialize)]
pub struct MinigameCampaignStageConfig {
    pub guid: i32,
    pub name_id: u32,
    pub description_id: u32,
    pub stage_icon_id: u32,
    pub start_screen_icon_id: u32,
    pub min_players: u32,
    pub max_players: u32,
    pub difficulty: u32,
    pub start_sound_id: u32,
    pub required_item_guid: Option<u32>,
    pub members_only: bool,
    #[serde(default = "default_true")]
    pub require_previous_completed: bool,
    pub link_name: String,
    pub short_name: String,
    pub minigame_type: MinigameType,
    pub zone_template_guid: u8,
    pub score_to_credits_expression: String,
    #[serde(default = "default_matchmaking_timeout_millis")]
    pub matchmaking_timeout_millis: u32,
    pub single_player_stage_guid: Option<StageLocator>,
    #[serde(default)]
    pub challenges: Vec<MinigameChallengeConfig>,
}

impl MinigameCampaignStageConfig {
    pub fn has_completed(&self, player: &Player) -> bool {
        player.minigame_stats.has_completed(self.guid)
    }

    pub fn unlocked(&self, player: &Player, previous_completed: bool) -> bool {
        self.required_item_guid
            .map(|item_guid| player.inventory.contains(&item_guid))
            .unwrap_or(true)
            && (previous_completed || !self.require_previous_completed)
            && (!self.members_only || player.member)
    }

    pub fn to_stage_definition(&self, portal_entry_guid: u32) -> MinigameStageDefinition {
        MinigameStageDefinition {
            guid: self.guid,
            portal_entry_guid,
            start_screen_name_id: self.name_id,
            start_screen_description_id: self.description_id,
            start_screen_icon_id: self.start_screen_icon_id,
            difficulty: self.difficulty,
            members_only: self.members_only,
            unknown8: 0,
            unknown9: "".to_string(),
            unknown10: 0,
            unknown11: 0,
            start_sound_id: self.start_sound_id,
            unknown13: "".to_string(),
            unknown14: 0,
            unknown15: 0,
            unknown16: 0,
        }
    }

    pub fn to_stage_instance(
        &self,
        portal_entry_guid: u32,
        stage_number: u32,
        player: &Player,
        previous_completed: bool,
    ) -> MinigameStageInstance {
        MinigameStageInstance {
            stage_instance_guid: self.guid,
            portal_entry_guid,
            link_name: self.link_name.clone(),
            short_name: self.short_name.clone(),
            unlocked: self.unlocked(player, previous_completed),
            unknown6: 0,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_id: self.stage_icon_id,
            parent_stage_instance_guid: 0,
            members_only: self.members_only,
            unknown12: 0,
            background_swf: "".to_string(),
            min_players: self.min_players,
            max_players: self.max_players,
            stage_number,
            required_item_id: 0,
            unknown18: 0,
            completed: self.has_completed(player),
            stage_group_instance_guid: 0,
        }
    }
}

#[derive(Deserialize)]
pub enum MinigameStageGroupChild {
    StageGroup(Arc<MinigameStageGroupConfig>),
    Stage(MinigameCampaignStageConfig),
}

const fn default_true() -> bool {
    true
}

const fn default_matchmaking_timeout_millis() -> u32 {
    10000
}

#[derive(Deserialize)]
pub struct MinigameStageGroupConfig {
    pub guid: i32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_id: u32,
    pub stage_icon_id: u32,
    pub stage_select_map_name: String,
    pub required_item_guid: Option<u32>,
    pub members_only: bool,
    #[serde(default = "default_true")]
    pub require_previous_completed: bool,
    #[serde(default)]
    pub short_name: String,
    #[serde(default)]
    pub default_stage_instance: i32,
    pub stages: Vec<MinigameStageGroupChild>,
}

impl MinigameStageGroupConfig {
    pub fn has_completed_any(&self, player: &Player) -> bool {
        self.stages
            .iter()
            .any(|child: &MinigameStageGroupChild| match child {
                MinigameStageGroupChild::StageGroup(stage_group) => {
                    stage_group.has_completed_any(player)
                }
                MinigameStageGroupChild::Stage(stage) => stage.has_completed(player),
            })
    }

    pub fn unlocked(&self, player: &Player, previous_completed: bool) -> bool {
        self.required_item_guid
            .map(|item_guid| player.inventory.contains(&item_guid))
            .unwrap_or(true)
            && (previous_completed || !self.require_previous_completed)
            && (!self.members_only || player.member)
    }

    pub fn to_stage_group_definition(
        &self,
        portal_entry_guid: u32,
    ) -> (
        Vec<MinigameStageGroupDefinition>,
        Vec<MinigameStageDefinition>,
    ) {
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();
        let mut group_links = Vec::new();

        for (index, child) in self.stages.iter().enumerate() {
            let stage_number = index as u32 + 1;
            match child {
                MinigameStageGroupChild::StageGroup(stage_group) => {
                    let (mut stage_group_definitions, mut stage_definitions) =
                        stage_group.to_stage_group_definition(portal_entry_guid);
                    stage_groups.append(&mut stage_group_definitions);
                    stages.append(&mut stage_definitions);

                    group_links.push(MinigameStageGroupLink {
                        link_id: 0,
                        parent_stage_group_definition_guid: self.guid,
                        parent_stage_definition_guid: 0,
                        child_stage_definition_guid: 0,
                        icon_id: 0,
                        link_name: GROUP_LINK_NAME.to_string(),
                        short_name: stage_group.short_name.clone(),
                        stage_number,
                        child_stage_group_definition_guid: stage_group.guid,
                    });
                }
                MinigameStageGroupChild::Stage(stage) => {
                    stages.push(stage.to_stage_definition(portal_entry_guid));
                    group_links.push(MinigameStageGroupLink {
                        link_id: 0,
                        parent_stage_group_definition_guid: self.guid,
                        parent_stage_definition_guid: 0,
                        child_stage_definition_guid: stage.guid,
                        icon_id: 0,
                        link_name: stage.link_name.clone(),
                        short_name: stage.short_name.clone(),
                        stage_number,
                        child_stage_group_definition_guid: 0,
                    });

                    stage.challenges.iter().for_each(|challenge| {
                        stages.push(challenge.to_stage_definition(portal_entry_guid, stage));
                        group_links.push(MinigameStageGroupLink {
                            link_id: 0,
                            parent_stage_group_definition_guid: self.guid,
                            parent_stage_definition_guid: 0,
                            child_stage_definition_guid: challenge.guid,
                            icon_id: 0,
                            link_name: CHALLENGE_LINK_NAME.to_string(),
                            short_name: "".to_string(),
                            stage_number,
                            child_stage_group_definition_guid: 0,
                        });
                    });
                }
            }
        }

        stage_groups.push(MinigameStageGroupDefinition {
            guid: self.guid,
            portal_entry_guid,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_id: self.icon_id,
            stage_select_map_name: self.stage_select_map_name.clone(),
            stage_progression: "".to_string(),
            show_start_screen_on_play_next: false,
            settings_icon_id: 0,
            opened_from_portal_entry_guid: portal_entry_guid,
            required_item_id: 0,
            required_bundle_id: 0,
            required_prereq_item_id: 0,
            group_links,
        });

        (stage_groups, stages)
    }

    pub fn to_stage_group_instance(
        &self,
        portal_entry_guid: u32,
        player: &Player,
    ) -> CreateMinigameStageGroupInstance {
        let mut stage_instances = Vec::new();
        let mut previous_completed = true;

        for (index, child) in self.stages.iter().enumerate() {
            let stage_number = index as u32 + 1;
            match child {
                MinigameStageGroupChild::StageGroup(stage_group) => {
                    let unlocked = stage_group.unlocked(player, previous_completed);
                    previous_completed = stage_group.has_completed_any(player);

                    stage_instances.push(MinigameStageInstance {
                        stage_instance_guid: 0,
                        portal_entry_guid,
                        link_name: GROUP_LINK_NAME.to_string(),
                        short_name: stage_group.short_name.clone(),
                        unlocked,
                        unknown6: 0,
                        name_id: stage_group.name_id,
                        description_id: stage_group.description_id,
                        icon_id: stage_group.stage_icon_id,
                        parent_stage_instance_guid: 0,
                        members_only: stage_group.members_only,
                        unknown12: 0,
                        background_swf: "".to_string(),
                        min_players: 0,
                        max_players: 0,
                        stage_number,
                        required_item_id: 0,
                        unknown18: 0,
                        completed: previous_completed,
                        stage_group_instance_guid: stage_group.guid,
                    });
                }
                MinigameStageGroupChild::Stage(stage) => {
                    let stage_instance = stage.to_stage_instance(
                        portal_entry_guid,
                        stage_number,
                        player,
                        previous_completed,
                    );
                    previous_completed = stage_instance.completed;

                    for challenge in &stage.challenges {
                        stage_instances.push(challenge.to_stage_instance(
                            portal_entry_guid,
                            player,
                            &stage_instance,
                        ));
                    }
                    stage_instances.push(stage_instance);
                }
            }
        }

        CreateMinigameStageGroupInstance {
            header: MinigameHeader {
                stage_guid: -1,
                sub_op_code: -1,
                stage_group_guid: self.guid,
            },
            stage_group_guid: self.guid,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_id: self.icon_id,
            stage_select_map_name: self.stage_select_map_name.clone(),
            default_stage_instance_guid: self.default_stage_instance,
            stage_instances,
            stage_progression: "".to_string(),
            show_start_screen_on_play_next: false,
            settings_icon_id: 0,
        }
    }
}

#[derive(Deserialize)]
pub struct MinigamePortalEntryConfig {
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub members_only: bool,
    pub is_flash: bool,
    pub is_active: bool,
    pub param1: u32,
    pub icon_id: u32,
    pub background_icon_id: u32,
    pub is_popular: bool,
    pub is_game_of_day: bool,
    pub sort_order: u32,
    pub tutorial_swf: String,
    pub stage_group: Arc<MinigameStageGroupConfig>,
}

impl MinigamePortalEntryConfig {
    pub fn to_portal_entry(
        &self,
        portal_category_guid: u32,
    ) -> (
        MinigamePortalEntry,
        Vec<MinigameStageGroupDefinition>,
        Vec<MinigameStageDefinition>,
    ) {
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();

        let (mut stage_group_definitions, mut stage_definitions) =
            self.stage_group.to_stage_group_definition(self.guid);
        stage_groups.append(&mut stage_group_definitions);
        stages.append(&mut stage_definitions);

        (
            MinigamePortalEntry {
                guid: self.guid,
                name_id: self.name_id,
                description_id: self.description_id,
                members_only: self.members_only,
                is_flash: self.is_flash,
                is_micro: false,
                is_active: self.is_active,
                param1: self.param1,
                icon_id: self.icon_id,
                background_icon_id: self.background_icon_id,
                is_popular: self.is_popular,
                is_game_of_day: self.is_game_of_day,
                portal_category_guid,
                sort_order: self.sort_order,
                tutorial_swf: self.tutorial_swf.clone(),
            },
            stage_groups,
            stages,
        )
    }
}

#[derive(Deserialize)]
pub struct MinigamePortalCategoryConfig {
    pub guid: u32,
    pub name_id: u32,
    pub icon_id: u32,
    pub sort_order: u32,
    pub portal_entries: Vec<MinigamePortalEntryConfig>,
}

impl From<&MinigamePortalCategoryConfig>
    for (
        MinigamePortalCategory,
        Vec<MinigamePortalEntry>,
        Vec<MinigameStageGroupDefinition>,
        Vec<MinigameStageDefinition>,
    )
{
    fn from(value: &MinigamePortalCategoryConfig) -> Self {
        let mut entries = Vec::new();
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();

        for entry in &value.portal_entries {
            let (entry_definition, mut stage_group_definitions, mut stage_definitions) =
                entry.to_portal_entry(value.guid);
            entries.push(entry_definition);
            stage_groups.append(&mut stage_group_definitions);
            stages.append(&mut stage_definitions);
        }

        (
            MinigamePortalCategory {
                guid: value.guid,
                name_id: value.name_id,
                icon_id: value.icon_id,
                sort_order: value.sort_order,
            },
            entries,
            stage_groups,
            stages,
        )
    }
}

impl From<&[MinigamePortalCategoryConfig]> for MinigameDefinitions {
    fn from(value: &[MinigamePortalCategoryConfig]) -> Self {
        let mut portal_categories = Vec::new();
        let mut portal_entries = Vec::new();
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();

        for category in value {
            let (
                category_definition,
                mut entry_definitions,
                mut stage_group_definitions,
                mut stage_definitions,
            ) = category.into();
            portal_categories.push(category_definition);
            portal_entries.append(&mut entry_definitions);
            stage_groups.append(&mut stage_group_definitions);
            stages.append(&mut stage_definitions);
        }

        MinigameDefinitions {
            header: MinigameHeader {
                stage_guid: -1,
                sub_op_code: -1,
                stage_group_guid: -1,
            },
            stages,
            stage_groups,
            portal_entries,
            portal_categories,
        }
    }
}

pub enum MinigameStageConfig<'a> {
    CampaignStage(&'a MinigameCampaignStageConfig),
    Challenge(&'a MinigameChallengeConfig, &'a MinigameCampaignStageConfig),
}

impl MinigameStageConfig<'_> {
    pub fn guid(&self) -> i32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.guid,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.guid,
        }
    }

    pub fn max_players(&self) -> u32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.max_players,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.max_players,
        }
    }

    pub fn min_players(&self) -> u32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.min_players,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.min_players,
        }
    }

    pub fn zone_template_guid(&self) -> u8 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.zone_template_guid,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.zone_template_guid,
        }
    }

    pub fn minigame_type(&self) -> &MinigameType {
        match self {
            MinigameStageConfig::CampaignStage(stage) => &stage.minigame_type,
            MinigameStageConfig::Challenge(challenge, ..) => &challenge.minigame_type,
        }
    }

    pub fn matchmaking_timeout_millis(&self) -> u32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.matchmaking_timeout_millis,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.matchmaking_timeout_millis,
        }
    }

    pub fn single_player_stage_guid(&self) -> Option<StageLocator> {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.single_player_stage_guid,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.single_player_stage_guid,
        }
    }

    pub fn score_to_credits_expression(&self) -> &String {
        match self {
            MinigameStageConfig::CampaignStage(stage) => &stage.score_to_credits_expression,
            MinigameStageConfig::Challenge(challenge, ..) => &challenge.score_to_credits_expression,
        }
    }

    pub fn name_id(&self) -> u32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.name_id,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.name_id,
        }
    }

    pub fn description_id(&self) -> u32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.description_id,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.description_id,
        }
    }

    pub fn start_screen_icon_id(&self) -> u32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.start_screen_icon_id,
            MinigameStageConfig::Challenge(_, base_stage) => base_stage.start_screen_icon_id,
        }
    }

    pub fn difficulty(&self) -> u32 {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.difficulty,
            MinigameStageConfig::Challenge(_, base_stage) => base_stage.difficulty,
        }
    }
}

pub struct StageConfigRef<'a> {
    pub stage_config: MinigameStageConfig<'a>,
    pub stage_number: u32,
    pub stage_group_guid: i32,
    pub portal_entry_guid: u32,
}

pub struct AllMinigameConfigs {
    categories: Vec<MinigamePortalCategoryConfig>,
    stage_groups: BTreeMap<i32, (Arc<MinigameStageGroupConfig>, u32)>,
}

impl AllMinigameConfigs {
    pub fn definitions(&self) -> MinigameDefinitions {
        (&self.categories[..]).into()
    }

    pub fn stage_group_instance(
        &self,
        stage_group_guid: i32,
        player: &Player,
    ) -> Result<CreateMinigameStageGroupInstance, ProcessPacketError> {
        if let Some((stage_group, portal_entry_guid)) = self.stage_groups.get(&stage_group_guid) {
            Ok(stage_group.to_stage_group_instance(*portal_entry_guid, player))
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Requested unknown stage group instance {}",
                    stage_group_guid
                ),
            ))
        }
    }

    pub fn stage_configs(&self) -> impl Iterator<Item = StageConfigRef> {
        self.stage_groups
            .values()
            .flat_map(|(stage_group, portal_entry_guid)| {
                stage_group
                    .stages
                    .iter()
                    .enumerate()
                    .filter_map(|(index, child)| match child {
                        MinigameStageGroupChild::StageGroup(_) => None,
                        MinigameStageGroupChild::Stage(stage) => Some((index, stage)),
                    })
                    .flat_map(move |(index, stage)| {
                        let stage_number = index as u32 + 1;
                        iter::once(StageConfigRef {
                            stage_config: MinigameStageConfig::CampaignStage(stage),
                            stage_number,
                            stage_group_guid: stage_group.guid,
                            portal_entry_guid: *portal_entry_guid,
                        })
                        .chain(stage.challenges.iter().map(
                            move |challenge| StageConfigRef {
                                stage_config: MinigameStageConfig::Challenge(challenge, stage),
                                stage_number,
                                stage_group_guid: stage_group.guid,
                                portal_entry_guid: *portal_entry_guid,
                            },
                        ))
                    })
            })
    }

    pub fn stage_config(&self, stage_group_guid: i32, stage_guid: i32) -> Option<StageConfigRef> {
        self.stage_groups
            .get(&stage_group_guid)
            .and_then(|(stage_group, portal_entry_guid)| {
                stage_group
                    .stages
                    .iter()
                    .enumerate()
                    .find_map(|(index, child)| {
                        if let MinigameStageGroupChild::Stage(stage) = child {
                            let stage_number = index as u32 + 1;
                            if stage.guid == stage_guid {
                                Some(StageConfigRef {
                                    stage_config: MinigameStageConfig::CampaignStage(stage),
                                    stage_number,
                                    stage_group_guid: stage_group.guid,
                                    portal_entry_guid: *portal_entry_guid,
                                })
                            } else {
                                for challenge in &stage.challenges {
                                    if challenge.guid == stage_guid {
                                        return Some(StageConfigRef {
                                            stage_config: MinigameStageConfig::Challenge(
                                                challenge, stage,
                                            ),
                                            stage_number,
                                            stage_group_guid: stage_group.guid,
                                            portal_entry_guid: *portal_entry_guid,
                                        });
                                    }
                                }

                                None
                            }
                        } else {
                            None
                        }
                    })
            })
    }

    pub fn stage_unlocked(&self, stage_group_guid: i32, stage_guid: i32, player: &Player) -> bool {
        if let Some((root_stage_group, _)) = self.stage_groups.get(&stage_group_guid) {
            let mut previous_completed = true;

            for child in root_stage_group.stages.iter() {
                match child {
                    MinigameStageGroupChild::StageGroup(stage_group) => {
                        previous_completed = stage_group.has_completed_any(player);
                    }
                    MinigameStageGroupChild::Stage(stage) => {
                        let unlocked = stage.unlocked(player, previous_completed);
                        previous_completed = stage.has_completed(player);

                        if stage_guid == stage.guid {
                            return unlocked;
                        }

                        for challenge in &stage.challenges {
                            if stage_guid == challenge.guid {
                                return challenge.unlocked(player, previous_completed);
                            }
                        }
                    }
                }
            }
        }

        false
    }
}

fn insert_stage_groups(
    portal_entry_guid: u32,
    stage_group: &Arc<MinigameStageGroupConfig>,
    map: &mut BTreeMap<i32, (Arc<MinigameStageGroupConfig>, u32)>,
) {
    map.insert(stage_group.guid, (stage_group.clone(), portal_entry_guid));

    for child in &stage_group.stages {
        if let MinigameStageGroupChild::StageGroup(stage_group) = child {
            insert_stage_groups(portal_entry_guid, stage_group, map);
        }
    }
}

impl From<Vec<MinigamePortalCategoryConfig>> for AllMinigameConfigs {
    fn from(value: Vec<MinigamePortalCategoryConfig>) -> Self {
        let mut stage_groups = BTreeMap::new();
        for category in &value {
            for entry in &category.portal_entries {
                insert_stage_groups(entry.guid, &entry.stage_group, &mut stage_groups);
            }
        }

        AllMinigameConfigs {
            categories: value,
            stage_groups,
        }
    }
}

pub fn load_all_minigames(config_dir: &Path) -> Result<AllMinigameConfigs, Error> {
    let mut file = File::open(config_dir.join("minigames.json"))?;
    let configs: Vec<MinigamePortalCategoryConfig> = serde_json::from_reader(&mut file)?;
    Ok(configs.into())
}

pub fn process_minigame_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u8 = cursor.read_u8()?;
    match MinigameOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            MinigameOpCode::RequestMinigameStageGroupInstance => {
                let request = RequestMinigameStageGroupInstance::deserialize(cursor)?;
                handle_request_stage_group_instance(request, sender, game_server)
            }
            MinigameOpCode::RequestCreateActiveMinigame => {
                let request = RequestCreateActiveMinigame::deserialize(cursor)?;
                handle_request_create_active_minigame(request, sender, game_server)
            }
            MinigameOpCode::RequestStartActiveMinigame => {
                let request = RequestStartActiveMinigame::deserialize(cursor)?;
                handle_request_start_active_minigame(request, sender, game_server)
            }
            MinigameOpCode::RequestCancelActiveMinigame => {
                let request = RequestCancelActiveMinigame::deserialize(cursor)?;
                handle_request_cancel_active_minigame(&request.header, true, sender, game_server)
            }
            MinigameOpCode::FlashPayload => {
                let payload = FlashPayload::deserialize(cursor)?;
                handle_flash_payload(payload, sender, game_server)
            }
            MinigameOpCode::SaberStrike => process_saber_strike_packet(cursor, sender, game_server),
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!(
                        "Unimplemented minigame op code: {:?} {:x?}",
                        op_code, buffer
                    ),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown minigame packet: {}, {:x?}", raw_op_code, buffer),
            ))
        }
    }
}

fn handle_request_stage_group_instance(
    request: RequestMinigameStageGroupInstance,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: vec![player_guid(sender)],
            write_guids: Vec::new(),
            character_consumer: |_, characters_read, _, _| {
                if let Some(character_read_handle) = characters_read.get(&player_guid(sender)) {
                    if let CharacterType::Player(player) =
                        &character_read_handle.stats.character_type
                    {
                        Ok(vec![Broadcast::Single(
                            sender,
                            vec![
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: game_server.minigames().stage_group_instance(
                                        request.header.stage_group_guid,
                                        player,
                                    )?,
                                })?,
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: ShowStageInstanceSelect {
                                        header: MinigameHeader {
                                            stage_guid: -1,
                                            sub_op_code: -1,
                                            stage_group_guid: request.header.stage_group_guid,
                                        },
                                    },
                                })?,
                            ],
                        )])
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Non-player character {} requested a stage group instance {}",
                                sender, request.header.stage_group_guid
                            ),
                        ))
                    }
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Unknown character {} requested a stage group instance {}",
                            sender, request.header.stage_group_guid
                        ),
                    ))
                }
            },
        })
}

fn find_matchmaking_group(
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    required_space: u32,
    max_players: u32,
    stage_group_guid: i32,
    stage_guid: i32,
    start_time: Instant,
) -> Option<(CharacterMatchmakingGroupIndex, u32)> {
    let range = (
        MatchmakingGroupStatus::OpenToAll,
        stage_group_guid,
        stage_guid,
        start_time,
        u32::MIN,
    )
        ..=(
            MatchmakingGroupStatus::OpenToAll,
            stage_group_guid,
            stage_guid,
            Instant::now(),
            u32::MAX,
        );
    // Iterates from oldest group to newest groups, so groups waiting longer are prioritized first
    let mut group_to_join = None;
    for matchmaking_group in characters_table_write_handle.indices4_by_range(range) {
        let players_in_group = characters_table_write_handle
            .keys_by_index4(matchmaking_group)
            .count() as u32;
        if players_in_group <= max_players.saturating_sub(required_space) {
            group_to_join = Some((
                *matchmaking_group,
                max_players.saturating_sub(players_in_group),
            ));
        }
    }

    group_to_join
}

fn handle_request_create_active_minigame(
    request: RequestCreateActiveMinigame,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    if let Some(stage_config) = game_server
        .minigames()
        .stage_config(request.header.stage_group_guid, request.header.stage_guid)
    {
        game_server.lock_enforcer().write_characters(
            |characters_table_write_handle, zones_lock_enforcer| {
                zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                    if stage_config.stage_config.max_players() == 1 {
                        Ok(prepare_active_minigame_instance(
                            &[sender],
                            &stage_config,
                            characters_table_write_handle,
                            zones_table_write_handle,
                            None,
                            game_server,
                        ))
                    } else {
                        let mut broadcasts = vec![
                            Broadcast::Single(sender, vec![
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: SendStringId {
                                        sender_guid: player_guid(sender),
                                        message_id: 19149,
                                        is_anonymous: true,
                                        unknown2: false,
                                        is_action_bar_message: true,
                                        action_bar_text_color: ActionBarTextColor::Yellow,
                                        target_guid: 0,
                                        owner_guid: 0,
                                        unknown7: 0
                                    },
                                })?,
                            ],)
                        ];
                        let required_space = 1;
                        let (open_group, space_left) = find_matchmaking_group(
                            characters_table_write_handle,
                            required_space,
                            stage_config.stage_config.max_players(),
                            stage_config.stage_group_guid,
                            stage_config.stage_config.guid(),
                            game_server.start_time(),
                        )
                        .unwrap_or_else(|| {
                            (
                                (
                                    MatchmakingGroupStatus::OpenToAll,
                                    stage_config.stage_group_guid,
                                    stage_config.stage_config.guid(),
                                    Instant::now(),
                                    sender,
                                ),
                                stage_config.stage_config.max_players(),
                            )
                        });

                        characters_table_write_handle.update_value_indices(player_guid(sender), |possible_character_write_handle, _| {
                            if let Some(character_write_handle) = possible_character_write_handle {
                                if let CharacterType::Player(ref mut player) =
                                    &mut character_write_handle.stats.character_type
                                {
                                    player.matchmaking_group = Some(open_group);
                                    Ok(())
                                } else {
                                    Err(ProcessPacketError::new(
                                        ProcessPacketErrorType::ConstraintViolated,
                                        format!(
                                            "Character {} requested to join a stage {} but is not a player",
                                            player_guid(sender),
                                            stage_config.stage_config.guid()
                                        ),
                                    ))
                                }
                            } else {
                                Err(ProcessPacketError::new(
                                    ProcessPacketErrorType::ConstraintViolated,
                                    format!(
                                        "Character {} requested to join a stage {} but does not exist",
                                        player_guid(sender),
                                        stage_config.stage_config.guid()
                                    ),
                                ))
                            }
                        })?;

                        if space_left <= required_space {
                            let players_in_group: Vec<u32> = characters_table_write_handle
                                .keys_by_index4(&open_group)
                                .filter_map(|guid| shorten_player_guid(guid).ok())
                                .collect();
                            broadcasts.append(&mut prepare_active_minigame_instance(
                                &players_in_group,
                                &stage_config,
                                characters_table_write_handle,
                                zones_table_write_handle,
                                None,
                                game_server,
                            ));
                        }

                        Ok(broadcasts)
                    }
                })
            },
        )
    } else {
        Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {} requested to join stage {} in stage group {}, but it doesn't exist",
                sender, request.header.stage_guid, request.header.stage_group_guid
            ),
        ))
    }
}

pub fn remove_from_matchmaking(
    player: u32,
    stage_group_guid: i32,
    stage_guid: i32,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    was_teleported: bool,
    message_id: Option<u32>,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let previous_location = characters_table_write_handle.update_value_indices(player_guid(player), |possible_character_write_handle, _| {
        if let Some(character_write_handle) = possible_character_write_handle {
            if let CharacterType::Player(player) =
                &mut character_write_handle.stats.character_type
            {
                let previous_location = player.previous_location.clone();
                player.matchmaking_group = None;
                Ok(previous_location)
            } else {
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to remove player {} from matchmaking, but their character isn't a player",
                        player
                    ),
                ))
            }
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Tried to end unknown player {}'s active minigame", player),
            ))
        }
    })?;

    let mut broadcasts = Vec::new();
    let mut result_packets = vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: ActiveMinigameCreationResult {
            header: MinigameHeader {
                stage_guid,
                sub_op_code: -1,
                stage_group_guid,
            },
            was_successful: false,
        },
    })?];
    if let Some(message) = message_id {
        result_packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: SendStringId {
                sender_guid: player_guid(player),
                message_id: message,
                is_anonymous: true,
                unknown2: false,
                is_action_bar_message: true,
                action_bar_text_color: ActionBarTextColor::Yellow,
                target_guid: 0,
                owner_guid: 0,
                unknown7: 0,
            },
        })?);
    }
    broadcasts.push(Broadcast::Single(player, result_packets));

    if was_teleported {
        let instance_guid = game_server.get_or_create_instance(
            characters_table_write_handle,
            zones_table_write_handle,
            previous_location.template_guid,
            1,
        )?;

        let teleport_result: Result<Vec<Broadcast>, ProcessPacketError> = teleport_to_zone!(
            characters_table_write_handle,
            player,
            zones_table_write_handle,
            &zones_table_write_handle
                .get(instance_guid)
                .unwrap_or_else(|| panic!(
                    "Zone instance {} should have been created or already exist but is missing",
                    instance_guid
                ))
                .read(),
            Some(previous_location.pos),
            Some(previous_location.rot),
            game_server.mounts(),
        );
        broadcasts.append(&mut teleport_result?);
    }

    Ok(broadcasts)
}

pub fn prepare_active_minigame_instance(
    members: &[u32],
    stage_config: &StageConfigRef,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    message_id: Option<u32>,
    game_server: &GameServer,
) -> Vec<Broadcast> {
    let stage_group_guid = stage_config.stage_group_guid;
    let stage_guid = stage_config.stage_config.guid();

    let mut broadcasts = Vec::new();

    let mut teleported_players = BTreeSet::new();
    let teleport_result: Result<Vec<Broadcast>, ProcessPacketError> = (|| {
        let new_instance_guid = game_server.get_or_create_instance(
            characters_table_write_handle,
            zones_table_write_handle,
            stage_config.stage_config.zone_template_guid(),
            stage_config.stage_config.max_players(),
        )?;

        let mut teleport_broadcasts = Vec::new();
        let now = Instant::now();
        for member_guid in members {
            characters_table_write_handle.update_value_indices(player_guid(*member_guid), |possible_character_write_handle, _| {
                if let Some(character_write_handle) = possible_character_write_handle {
                    if let CharacterType::Player(player) = &mut character_write_handle.stats.character_type {
                        if game_server.minigames().stage_unlocked(stage_group_guid, stage_guid, player) {
                            player.minigame_status = Some(MinigameStatus {
                                stage_group_guid,
                                stage_guid,
                                game_created: false,
                                game_won: false,
                                score_entries: vec![],
                                total_score: 0,
                                awarded_credits: 0,
                                start_time: now,
                                type_data: stage_config.stage_config.minigame_type().into(),
                            });
                            if let Some(group) = player.matchmaking_group.as_mut() {
                                group.0 = MatchmakingGroupStatus::Closed;
                            }
                            Ok(())
                        } else {
                            Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} tried to create an active minigame for a stage {} they haven't unlocked", member_guid, stage_guid)))
                        }
                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} tried to create an active minigame, but their character isn't a player", member_guid)))
                    }
                } else {
                    Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} tried to create an active minigame", member_guid)))
                }
            })?;

            if let Some(message) = message_id {
                let string_id_packet_result = GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: SendStringId {
                        sender_guid: player_guid(*member_guid),
                        message_id: message,
                        is_anonymous: true,
                        unknown2: false,
                        is_action_bar_message: true,
                        action_bar_text_color: ActionBarTextColor::Yellow,
                        target_guid: 0,
                        owner_guid: 0,
                        unknown7: 0,
                    },
                });

                match string_id_packet_result {
                    Ok(packet) => {
                        teleport_broadcasts.push(Broadcast::Single(*member_guid, vec![packet]))
                    }
                    Err(err) => info!(
                        "Couldn't serialize send string packet: {} (stage group {}, stage {})",
                        ProcessPacketError::from(err),
                        stage_group_guid,
                        stage_guid
                    ),
                }
            }

            let result: Result<Vec<Broadcast>, ProcessPacketError> = teleport_to_zone!(
                characters_table_write_handle,
                *member_guid,
                zones_table_write_handle,
                &zones_table_write_handle
                    .get(new_instance_guid)
                    .unwrap_or_else(|| panic!(
                        "Zone instance {} should have been created or already exist but is missing",
                        new_instance_guid
                    ))
                    .read(),
                None,
                None,
                game_server.mounts(),
            );
            // Only mark player as teleported if the teleportation was successful
            teleport_broadcasts.append(&mut result?);
            teleported_players.insert(*member_guid);
        }

        Ok(teleport_broadcasts)
    })();

    match teleport_result {
        Ok(mut teleport_broadcasts) => broadcasts.append(&mut teleport_broadcasts),
        Err(err) => {
            // We don't need to clean up the zone here, since the next instance of this stage that starts will use it instead
            info!("Couldn't add a player to the minigame, ending the game: {} (stage group {}, stage {})", err, stage_group_guid, stage_guid);
            for member_guid in members {
                let was_teleported = teleported_players.contains(member_guid);
                let end_matchmaking_result = remove_from_matchmaking(
                    *member_guid,
                    stage_group_guid,
                    stage_guid,
                    characters_table_write_handle,
                    zones_table_write_handle,
                    was_teleported,
                    None,
                    game_server,
                );
                if let Ok(mut end_game_broadcasts) = end_matchmaking_result {
                    broadcasts.append(&mut end_game_broadcasts);
                } else {
                    info!(
                        "Couldn't end minigame for player {}: {} (stage group {}, stage {})",
                        *member_guid, err, stage_group_guid, stage_guid
                    );
                }
            }
        }
    }

    // We don't want to return a `Result` because an error would disconnect the sender without disconnecting the group members
    broadcasts
}

fn handle_request_start_active_minigame(
    request: RequestStartActiveMinigame,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
        read_guids: vec![player_guid(sender)],
        write_guids: Vec::new(),
        character_consumer: |_, characters_read, _, _| {
            if let Some(character_read_handle) = characters_read.get(&player_guid(sender)) {
                if let CharacterType::Player(player) = &character_read_handle.stats.character_type {
                    if let Some(minigame_status) = &player.minigame_status {
                        if request.header.stage_guid == minigame_status.stage_guid {
                            let mut packets = Vec::new();

                            if let Some(StageConfigRef {stage_config, ..}) = game_server.minigames().stage_config(minigame_status.stage_group_guid, minigame_status.stage_guid) {
                                let mut stage_group_instance =
                                    game_server.minigames().stage_group_instance(minigame_status.stage_group_guid, player)?;
                                stage_group_instance.header.stage_guid = minigame_status.stage_guid;
                                // The default stage instance must be set for the how-to button the options menu to work
                                stage_group_instance.default_stage_instance_guid = minigame_status.stage_guid;

                                // Re-send the stage group instance to populate the stage data in the settings menu.
                                // When we enter the Flash or 3D game HUD state, the current minigame group is cleared.
                                // This removes the game name from the options menu. To avoid this, we need to send a 
                                // script packet to transition the HUD to the main state. Then we re-send the stage 
                                // group instance data. Then we can load the minigame.
                                packets.push(GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: ExecuteScriptWithStringParams {
                                        script_name: "UIGlobal.SetStateMain".to_string(),
                                        params: vec![],
                                    },
                                })?);
                                packets.push(GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: stage_group_instance,
                                })?);

                                match stage_config.minigame_type() {
                                    MinigameType::Flash { game_swf_name } => {
                                        packets.push(
                                            GamePacket::serialize(&TunneledPacket {
                                                unknown1: true,
                                                inner: StartFlashGame {
                                                    loader_script_name: "MiniGameFlash".to_string(),
                                                    game_swf_name: game_swf_name.clone(),
                                                    is_micro: false,
                                                },
                                            })?
                                        );
                                    },
                                    MinigameType::SaberStrike { saber_strike_stage_id } => {
                                        packets.push(
                                            GamePacket::serialize(&TunneledPacket {
                                                unknown1: true,
                                                inner: SaberStrikeStageData {
                                                    minigame_header: MinigameHeader {
                                                        stage_guid: minigame_status.stage_guid,
                                                        sub_op_code: SaberStrikeOpCode::StageData as i32,
                                                        stage_group_guid: minigame_status.stage_group_guid,
                                                    },
                                                    saber_strike_stage_id: *saber_strike_stage_id,
                                                    use_player_weapon: player.battle_classes.get(&player.active_battle_class)
                                                        .and_then(|battle_class| battle_class.items.get(&EquipmentSlot::PrimaryWeapon)
                                                        .and_then(|item| game_server.items().get(&item.guid)))
                                                        .map(|item| item.item_type == SABER_ITEM_TYPE)
                                                        .unwrap_or(false),
                                                }
                                            })?,
                                        );
                                    },
                                }
                            } else {
                                return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} requested to start active minigame with stage config {} (stage group {}) that does not exist", sender, minigame_status.stage_guid, minigame_status.stage_group_guid)));
                            }

                            packets.push(
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: StartActiveMinigame {
                                        header: MinigameHeader {
                                            stage_guid: minigame_status.stage_guid,
                                            sub_op_code: -1,
                                            stage_group_guid: minigame_status.stage_group_guid,
                                        },
                                    },
                                })?,
                            );

                            Ok(vec![
                                Broadcast::Single(sender, packets)
                            ])
                        } else {
                            Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} requested to start an active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})", sender, request.header.stage_guid, minigame_status.stage_group_guid, minigame_status.stage_guid)))
                        }
                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} requested to start an active minigame (stage {}), but they aren't in an active minigame", sender, request.header.stage_guid)))
                    }
                } else {
                    Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} requested to start an active minigame, but their character isn't a player", sender)))
                }
            } else {
                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} requested to start an active minigame", sender)))
            }
        }
    })
}

fn handle_request_cancel_active_minigame(
    request_header: &MinigameHeader,
    skip_if_flash: bool,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, zones_lock_enforcer| {
            zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                end_active_minigame(
                    sender,
                    characters_table_write_handle,
                    zones_table_write_handle,
                    request_header.stage_guid,
                    skip_if_flash,
                    game_server,
                )
            })
        },
    )
}

pub fn handle_minigame_packet_write<T: Default>(
    sender: u32,
    game_server: &GameServer,
    header: &MinigameHeader,
    func: impl FnOnce(
        &mut MinigameStatus,
        &mut PlayerMinigameStats,
        &mut u32,
        StageConfigRef,
    ) -> Result<T, ProcessPacketError>,
) -> Result<T, ProcessPacketError> {
    game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
        read_guids: Vec::new(),
        write_guids: vec![player_guid(sender)],
        character_consumer: |_, _, mut characters_write, _|  {
            if let Some(character_write_handle) =
                characters_write.get_mut(&player_guid(sender))
            {
                if let CharacterType::Player(player) = &mut character_write_handle.stats.character_type {
                    if let Some(minigame_status) = &mut player.minigame_status {
                        if header.stage_guid == minigame_status.stage_guid {
                            if let Some(stage_config_ref) = game_server
                                .minigames()
                                .stage_config(minigame_status.stage_group_guid, minigame_status.stage_guid)
                            {
                                Ok(func(minigame_status, &mut player.minigame_stats, &mut player.credits, stage_config_ref)?)
                            } else {
                                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to process Flash payload for {}'s active minigame with stage config {} (stage group {}) that does not exist", sender, minigame_status.stage_guid, minigame_status.stage_group_guid)))
                            }
                        } else {
                            Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to process packet for {}'s active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})", sender, header.stage_guid, minigame_status.stage_group_guid, minigame_status.stage_guid)))
                        }
                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to process packet for {}'s active minigame (stage {}), but they aren't in an active minigame", sender, header.stage_guid)))
                    }
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Tried to process packet for {}'s active minigame, but their character isn't a player",
                            sender
                        ),
                    ))
                }
            } else {
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Tried to process packet for unknown player {}'s active minigame", sender),
                ))
            }
        },
    })
}

fn handle_flash_payload_read_only<T: Default>(
    sender: u32,
    game_server: &GameServer,
    header: &MinigameHeader,
    func: impl FnOnce(&MinigameStatus, StageConfigRef) -> Result<T, ProcessPacketError>,
) -> Result<T, ProcessPacketError> {
    game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
        read_guids: vec![player_guid(sender)],
        write_guids: Vec::new(),
        character_consumer: |_, characters_read, _, _|  {
            if let Some(character_read_handle) =
                characters_read.get(&player_guid(sender))
            {
                if let CharacterType::Player(player) = &character_read_handle.stats.character_type {
                    if let Some(minigame_status) = &player.minigame_status {
                        if header.stage_guid == minigame_status.stage_guid {
                            if let Some(stage_config_ref) = game_server
                                .minigames()
                                .stage_config(minigame_status.stage_group_guid, minigame_status.stage_guid)
                            {
                                Ok(func(minigame_status, stage_config_ref)?)
                            } else {
                                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to process Flash payload for {}'s active minigame with stage config {} (stage group {}) that does not exist", sender, minigame_status.stage_guid, minigame_status.stage_group_guid)))
                            }
                        } else {
                            Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to process Flash payload for {}'s active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})", sender, header.stage_guid, minigame_status.stage_group_guid, minigame_status.stage_guid)))
                        }
                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to process Flash payload for {}'s active minigame (stage {}), but they aren't in an active minigame", sender, header.stage_guid)))
                    }
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Tried to process Flash payload for {}'s active minigame, but their character isn't a player",
                            sender
                        ),
                    ))
                }
            } else {
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Tried to process Flash payload for unknown player {}'s active minigame", sender),
                ))
            }
        },
    })
}

fn handle_flash_payload_win(
    parts: &[&str],
    sender: u32,
    payload: &FlashPayload,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    handle_minigame_packet_write(
        sender,
        game_server,
        &payload.header,
        |minigame_status, minigame_stats, _, _| {
            if parts.len() == 2 {
                let total_score = parts[1].parse()?;
                minigame_status.total_score = total_score;
                minigame_status.score_entries.push(ScoreEntry {
                    entry_text: "".to_string(),
                    icon_set_id: 0,
                    score_type: ScoreType::Total,
                    score_count: total_score,
                    score_max: 0,
                    score_points: 0,
                });
                minigame_status.game_won = true;

                minigame_stats.complete(minigame_status.stage_guid, total_score);

                Ok(vec![Broadcast::Single(
                    sender,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: FlashPayload {
                            header: MinigameHeader {
                                stage_guid: minigame_status.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.stage_group_guid,
                            },
                            payload: format!(
                                "OnGamePlayTimeMsg\t{}",
                                Instant::now()
                                    .duration_since(minigame_status.start_time)
                                    .as_millis()
                            ),
                        },
                    })?],
                )])
            } else {
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Expected 1 parameter in game won payload, but only found {}",
                        parts.len().saturating_sub(1)
                    ),
                ))
            }
        },
    )
}

fn handle_flash_payload(
    payload: FlashPayload,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let parts: Vec<&str> = payload.payload.split('\t').collect();
    if parts.is_empty() {
        return Ok(vec![]);
    }

    match parts[0] {
        "FRServer_RequestStageId" => handle_flash_payload_read_only(
            sender,
            game_server,
            &payload.header,
            |minigame_status, stage_config_ref| {
                Ok(vec![Broadcast::Single(
                    sender,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: FlashPayload {
                            header: MinigameHeader {
                                stage_guid: minigame_status.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.stage_group_guid,
                            },
                            payload: format!(
                                "VOnServerSetStageIdMsg\t{}",
                                stage_config_ref.stage_number
                            ),
                        },
                    })?],
                )])
            },
        ),
        "FRServer_ScoreInfo" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, _, _| {
                if parts.len() == 7 {
                    let icon_set_id = parts[2].parse()?;
                    let score_type = ScoreType::try_from_primitive(parts[3].parse()?)
                        .unwrap_or(ScoreType::Counter);
                    let score_count = parts[4].parse()?;
                    let score_max = parts[5].parse()?;
                    let score_points = parts[6].parse()?;

                    if score_type == ScoreType::Total {
                        minigame_status.total_score = score_count;
                    }

                    minigame_status.score_entries.push(ScoreEntry {
                        entry_text: parts[1].to_string(),
                        icon_set_id,
                        score_type,
                        score_count,
                        score_max,
                        score_points,
                    });
                    Ok(vec![])
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Expected 6 parameters in score info payload, but only found {}",
                            parts.len().saturating_sub(1)
                        ),
                    ))
                }
            },
        ),
        "FRServer_EndRoundNoValidation" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, player_credits, stage_config| {
                if parts.len() == 2 {
                    let round_score = parts[1].parse()?;
                    let (mut broadcasts, awarded_credits) = award_credits(
                        sender,
                        player_credits,
                        minigame_status,
                        &stage_config.stage_config,
                        round_score,
                    )?;

                    broadcasts.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: FlashPayload {
                                header: MinigameHeader {
                                    stage_guid: minigame_status.stage_guid,
                                    sub_op_code: -1,
                                    stage_group_guid: minigame_status.stage_group_guid,
                                },
                                payload: format!("OnShowEndRoundScreenMsg\t{}", awarded_credits),
                            },
                        })?],
                    ));

                    Ok(broadcasts)
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Expected 1 parameter in end round payload, but only found {}",
                            parts.len().saturating_sub(1)
                        ),
                    ))
                }
            },
        ),
        "FRServer_GameWon" => handle_flash_payload_win(&parts, sender, &payload, game_server),
        "FRServer_GameLost" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, _, _| {
                if parts.len() == 2 {
                    let total_score = parts[1].parse()?;
                    minigame_status.total_score = total_score;
                    minigame_status.score_entries.push(ScoreEntry {
                        entry_text: "".to_string(),
                        icon_set_id: 0,
                        score_type: ScoreType::Total,
                        score_count: total_score,
                        score_max: 0,
                        score_points: 0,
                    });
                    minigame_status.game_won = false;
                    Ok(vec![])
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Expected 1 parameter in game lost payload, but only found {}",
                            parts.len().saturating_sub(1)
                        ),
                    ))
                }
            },
        ),
        "FRServer_GameClose" => {
            handle_request_cancel_active_minigame(&payload.header, false, sender, game_server)
        }
        "FRServer_StatUpdate" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, minigame_stats, _, _| {
                if parts.len() == 3 {
                    let trophy_guid = parts[1].parse()?;
                    let delta = parts[2].parse()?;
                    minigame_stats.update_trophy_progress(trophy_guid, delta);
                    Ok(vec![])
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Expected 2 parameters in stat update payload, but only found {}",
                            parts.len().saturating_sub(1)
                        ),
                    ))
                }
            },
        ),
        _ => Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Received unknown Flash payload {} in stage {}, stage group {} from player {}",
                payload.payload, payload.header.stage_guid, payload.header.stage_group_guid, sender
            ),
        )),
    }
}

pub fn create_active_minigame(
    sender: u32,
    minigames: &AllMinigameConfigs,
    minigame_status: &MinigameStatus,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    if let Some(StageConfigRef {
        stage_config,
        portal_entry_guid,
        ..
    }) = minigames.stage_config(minigame_status.stage_group_guid, minigame_status.stage_guid)
    {
        Ok(vec![Broadcast::Single(
            sender,
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: ActiveMinigameCreationResult {
                        header: MinigameHeader {
                            stage_guid: minigame_status.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.stage_group_guid,
                        },
                        was_successful: true,
                    },
                })?,
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: CreateActiveMinigame {
                        header: MinigameHeader {
                            stage_guid: minigame_status.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.stage_group_guid,
                        },
                        name_id: stage_config.name_id(),
                        icon_set_id: stage_config.start_screen_icon_id(),
                        description_id: stage_config.description_id(),
                        difficulty: stage_config.difficulty(),
                        battle_class_type: 0,
                        portal_entry_guid,
                        unknown7: false,
                        unknown8: false,
                        reward_bundle1: RewardBundle::default(),
                        reward_bundle2: RewardBundle::default(),
                        reward_bundle3: RewardBundle::default(),
                        reward_bundles: vec![],
                        unknown13: false,
                        unknown14: false,
                        unknown15: false,
                        unknown16: false,
                        show_end_score_screen: true,
                        unknown18: "".to_string(),
                        unknown19: 0,
                        unknown20: false,
                        stage_definition_guid: minigame_status.stage_guid,
                        unknown22: false,
                        unknown23: false,
                        unknown24: false,
                        unknown25: 0,
                        unknown26: 0,
                        unknown27: 0,
                    },
                })?,
            ],
        )])
    } else {
        Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {} requested creation of unknown stage {} (stage group {})",
                sender, minigame_status.stage_guid, minigame_status.stage_group_guid
            ),
        ))
    }
}

fn award_credits(
    sender: u32,
    player_credits: &mut u32,
    minigame_status: &mut MinigameStatus,
    stage_config: &MinigameStageConfig,
    score: i32,
) -> Result<(Vec<Broadcast>, u32), ProcessPacketError> {
    let awarded_credits =
        evaluate_score_to_credits_expression(stage_config.score_to_credits_expression(), score)?
            .max(0) as u32;

    minigame_status.awarded_credits = minigame_status
        .awarded_credits
        .saturating_add(awarded_credits);

    let new_credits = player_credits.saturating_add(awarded_credits);
    *player_credits = new_credits;

    let broadcasts = vec![Broadcast::Single(
        sender,
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdateCredits { new_credits },
        })?],
    )];

    Ok((broadcasts, awarded_credits))
}

pub fn end_active_minigame(
    sender: u32,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    stage_guid: i32,
    skip_if_flash: bool,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let (mut broadcasts, previous_location, skip) = characters_table_write_handle.update_value_indices(player_guid(sender), |possible_character_write_handle, _| {
        if let Some(character_write_handle) = possible_character_write_handle{
            if let CharacterType::Player(player) = &mut character_write_handle.stats.character_type {
                let previous_location = player.previous_location.clone();

                if let Some(minigame_status) = &mut player.minigame_status {
                    if stage_guid == minigame_status.stage_guid {
                        if let Some(StageConfigRef { stage_config, .. }) = game_server
                            .minigames()
                            .stage_config(minigame_status.stage_group_guid, minigame_status.stage_guid)
                        {
                            // Wait for the end signal from the Flash payload because those games send additional score data
                            if skip_if_flash
                                && matches!(stage_config.minigame_type(), MinigameType::Flash { .. })
                            {
                                return Ok((Vec::new(), previous_location, true));
                            }

                            // If we've already awarded credits after a round, don't grant those credits again
                            let mut broadcasts = if minigame_status.awarded_credits > 0 {
                                Vec::new()
                            } else {
                                award_credits(
                                    sender,
                                    &mut player.credits,
                                    minigame_status,
                                    &stage_config,
                                    minigame_status.total_score,
                                )?
                                .0
                            };

                            let last_broadcast = Broadcast::Single(
                                sender,
                                vec![
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: FlashPayload {
                                            header: MinigameHeader {
                                                stage_guid: minigame_status.stage_guid,
                                                sub_op_code: -1,
                                                stage_group_guid: minigame_status.stage_group_guid,
                                            },
                                            payload: if minigame_status.game_won {
                                                "OnGameWonMsg".to_string()
                                            } else {
                                                "OnGameLostMsg".to_string()
                                            },
                                        },
                                    })?,
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: ActiveMinigameEndScore {
                                            header: MinigameHeader {
                                                stage_guid,
                                                sub_op_code: -1,
                                                stage_group_guid: minigame_status.stage_group_guid,
                                            },
                                            scores: minigame_status.score_entries.clone(),
                                            unknown2: true,
                                        },
                                    })?,
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: UpdateActiveMinigameRewards {
                                            header: MinigameHeader {
                                                stage_guid: minigame_status.stage_guid,
                                                sub_op_code: -1,
                                                stage_group_guid: minigame_status.stage_group_guid,
                                            },
                                            reward_bundle1: RewardBundle {
                                                unknown1: false,
                                                credits: minigame_status.awarded_credits,
                                                battle_class_xp: 0,
                                                unknown4: 0,
                                                unknown5: 0,
                                                unknown6: 0,
                                                unknown7: 0,
                                                unknown8: 0,
                                                unknown9: 0,
                                                unknown10: 0,
                                                unknown11: 0,
                                                unknown12: 0,
                                                unknown13: 0,
                                                icon_set_id: 0,
                                                name_id: 0,
                                                entries: vec![],
                                                unknown17: 0,
                                            },
                                            unknown1: 0,
                                            unknown2: 0,
                                            reward_bundle2: RewardBundle::default(),
                                            earned_trophies: vec![],
                                        },
                                    })?,
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: EndActiveMinigame {
                                            header: MinigameHeader {
                                                stage_guid: minigame_status.stage_guid,
                                                sub_op_code: -1,
                                                stage_group_guid: minigame_status.stage_group_guid,
                                            },
                                            won: minigame_status.game_won,
                                            unknown2: 0,
                                            unknown3: 0,
                                            unknown4: 0,
                                        },
                                    })?,
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: LeaveActiveMinigame {
                                            header: MinigameHeader {
                                                stage_guid: minigame_status.stage_guid,
                                                sub_op_code: -1,
                                                stage_group_guid: minigame_status.stage_group_guid,
                                            },
                                        },
                                    })?,
                                ],
                            );

                            broadcasts.push(Broadcast::Single(
                                sender,
                                vec![GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: game_server.minigames().stage_group_instance(
                                        minigame_status.stage_group_guid,
                                        player,
                                    )?,
                                })?],
                            ));
                            broadcasts.push(last_broadcast);

                            player.minigame_status = None;
                            player.matchmaking_group = None;

                            Ok((broadcasts, previous_location, false))
                        } else {
                            Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to end player {}'s active minigame with stage config {} (stage group {}) that does not exist", sender, minigame_status.stage_guid, minigame_status.stage_group_guid)))
                        }
                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to end player {}'s active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})", sender, stage_guid, minigame_status.stage_group_guid, minigame_status.stage_guid)))
                    }
                } else {
                    Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to end player {}'s active minigame (stage {}), but they aren't in an active minigame", sender, stage_guid)))
                }
            } else {
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to end player {}'s active minigame, but their character isn't a player",
                        sender
                    ),
                ))
            }
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Tried to end unknown player {}'s active minigame", sender),
            ))
        }
    })?;

    if skip {
        return Ok(Vec::new());
    }

    let instance_guid = game_server.get_or_create_instance(
        characters_table_write_handle,
        zones_table_write_handle,
        previous_location.template_guid,
        1,
    )?;
    let teleport_broadcasts: Result<Vec<Broadcast>, ProcessPacketError> = teleport_to_zone!(
        characters_table_write_handle,
        sender,
        zones_table_write_handle,
        &zones_table_write_handle
            .get(instance_guid)
            .unwrap_or_else(|| panic!(
                "Zone instance {} should have been created or already exist but is missing",
                instance_guid
            ))
            .read(),
        Some(previous_location.pos),
        Some(previous_location.rot),
        game_server.mounts(),
    );
    broadcasts.append(&mut teleport_broadcasts?);

    Ok(broadcasts)
}

fn evaluate_score_to_credits_expression(
    score_to_credits_expression: &str,
    score: i32,
) -> Result<i32, Error> {
    let context = context_map! {
        "x" => evalexpr::Value::Float(score as f64),
    }
    .unwrap_or_else(|_| {
        panic!(
            "Couldn't build expression evaluation context for score {}",
            score
        )
    });

    let result = eval_with_context(score_to_credits_expression, &context).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "Unable to evaluate score-to-credits expression for score {}: {}",
                score, err
            ),
        )
    })?;

    if let Value::Float(credits) = result {
        i32::try_from(credits.round() as i64).map_err(|err| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Score-to-credits expression returned float that could not be converted to an integer for score {}: {}, {}",
                    score,
                    credits,
                    err
                ),
            )
        })
    } else {
        Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Score-to-credits expression did not return an integer for score {}, returned: {}",
                score, result
            ),
        ))
    }
}

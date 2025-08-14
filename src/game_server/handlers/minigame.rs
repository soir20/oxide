use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Cursor, Error, ErrorKind, Read},
    iter,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use byteorder::ReadBytesExt;
use chrono::{DateTime, Datelike, FixedOffset, NaiveTime, Timelike, Utc};
use evalexpr::{context_map, eval_with_context, Value};
use num_enum::TryFromPrimitive;
use packet_serialize::DeserializePacket;
use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Deserializer};

use crate::{
    game_server::{
        handlers::{
            are_dates_in_same_week,
            character::{MinigameStatus, MinigameWinStatus},
            daily::{
                DailyHolocronGame, DailySpinGame, DailySpinRewardBucket, DailyTriviaGame,
                DailyTriviaQuestionConfig,
            },
            fleet_commander::FleetCommanderGame,
            force_connection::ForceConnectionGame,
            lock_enforcer::CharacterTableReadHandle,
        },
        packets::{
            chat::{ActionBarTextColor, SendStringId},
            client_update::UpdateCredits,
            command::StartFlashGame,
            daily::{AddDailyMinigame, UpdateDailyMinigame},
            item::EquipmentSlot,
            minigame::{
                ActiveMinigameCreationResult, ActiveMinigameEndScore, CreateActiveMinigame,
                CreateMinigameStageGroupInstance, EndActiveMinigame, FlashPayload,
                LeaveActiveMinigame, MinigameDefinitions, MinigameDefinitionsUpdate,
                MinigameHeader, MinigameOpCode, MinigamePortalCategory, MinigamePortalEntry,
                MinigameStageDefinition, MinigameStageGroupDefinition, MinigameStageGroupLink,
                MinigameStageInstance, RequestCancelActiveMinigame, RequestCreateActiveMinigame,
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
    info, teleport_to_zone, ConfigError,
};

use super::{
    character::{CharacterType, MinigameMatchmakingGroup, Player, PreviousLocation},
    guid::{GuidTableIndexer, IndexedGuid},
    item::SABER_ITEM_TYPE,
    lock_enforcer::{
        CharacterLockRequest, CharacterTableWriteHandle, MinigameDataLockRequest,
        MinigameDataTableWriteHandle, ZoneTableWriteHandle,
    },
    saber_strike::process_saber_strike_packet,
    unique_guid::{player_guid, shorten_player_guid},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MinigameBoost {
    Spin,
    Holocron,
    Trivia,
}

#[derive(Clone)]
pub struct PlayerStageStats {
    last_completion: Option<DateTime<FixedOffset>>,
    completions_this_week: [u8; 7],
    consecutive_days_completed: u32,
    high_score: i32,
}

#[derive(Clone, Default)]
pub struct PlayerMinigameStats {
    boosts: BTreeMap<MinigameBoost, u32>,
    stage_guid_to_stats: BTreeMap<i32, PlayerStageStats>,
    trophy_stats: BTreeMap<i32, i32>,
}

impl PlayerMinigameStats {
    pub fn boosts_remaining(&self, boost: MinigameBoost) -> u32 {
        self.boosts.get(&boost).cloned().unwrap_or(0)
    }

    pub fn use_boost(&mut self, boost: MinigameBoost) -> Result<u32, ProcessPacketError> {
        let Some(boosts_remaining) = self.boosts.get_mut(&boost) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player has no {boost:?} boosts"),
            ));
        };

        if *boosts_remaining == 0 {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player has no {boost:?} boosts"),
            ));
        }

        *boosts_remaining -= 1;
        Ok(*boosts_remaining)
    }

    pub fn complete(
        &mut self,
        stage_guid: i32,
        score: i32,
        win_time: DateTime<FixedOffset>,
        daily_reset_offset: &DailyResetOffset,
    ) {
        // Storing a count for each day of the week is more space-efficient than storing a list of times.
        // It could make the list slightly inaccurate if the reset time is changed, but that should be
        // rare enough to be an acceptable tradeoff.
        let day_of_week = win_time.weekday().num_days_from_sunday() as usize;
        self.stage_guid_to_stats
            .entry(stage_guid)
            .and_modify(|entry| {
                if let Some(last_completion) = entry.last_completion {
                    if !are_dates_in_same_week(&last_completion, &win_time, &daily_reset_offset.0) {
                        entry.completions_this_week = [0; 7];
                    }

                    let was_completed_yesterday = win_time.num_days_from_ce()
                        == last_completion.num_days_from_ce().saturating_add(1);
                    entry.consecutive_days_completed = match was_completed_yesterday {
                        true => entry.consecutive_days_completed.saturating_add(1),
                        false => 1,
                    };
                } else {
                    entry.consecutive_days_completed = 1;
                }

                entry.last_completion = Some(win_time);
                entry.completions_this_week[day_of_week] =
                    entry.completions_this_week[day_of_week].saturating_add(1);
                entry.high_score = score.max(entry.high_score);
            })
            .or_insert_with(|| {
                let mut completions_this_week = [0; 7];
                completions_this_week[day_of_week] = 1;

                PlayerStageStats {
                    last_completion: Some(win_time),
                    completions_this_week,
                    consecutive_days_completed: 1,
                    high_score: score,
                }
            });
    }

    pub fn completions_this_week(
        &self,
        stage_guid: i32,
        now: DateTime<FixedOffset>,
        daily_reset_offset: &DailyResetOffset,
    ) -> [u8; 7] {
        self.stage_guid_to_stats
            .get(&stage_guid)
            .map(|stats| {
                if let Some(last_completion) = stats.last_completion {
                    if !are_dates_in_same_week(&last_completion, &now, &daily_reset_offset.0) {
                        return [0; 7];
                    }
                }

                stats.completions_this_week
            })
            .unwrap_or_default()
    }

    pub fn last_completion_time(&self, stage_guid: i32) -> Option<DateTime<FixedOffset>> {
        self.stage_guid_to_stats
            .get(&stage_guid)
            .and_then(|stats| stats.last_completion)
    }

    pub fn has_completed(&self, stage_guid: i32) -> bool {
        self.last_completion_time(stage_guid).is_some()
    }

    pub fn consecutive_days_completed(&self, stage_guid: i32) -> u32 {
        self.stage_guid_to_stats
            .get(&stage_guid)
            .map(|stats| stats.consecutive_days_completed)
            .unwrap_or(0)
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
#[serde(deny_unknown_fields)]
pub struct StageLocator {
    pub stage_group_guid: i32,
    pub stage_guid: i32,
}

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FlashMinigameType {
    FleetCommander,
    ForceConnection,
    DailySpin {
        buckets: Vec<DailySpinRewardBucket>,
    },
    DailyHolocron,
    DailyTrivia {
        questions_per_game: u8,
        consecutive_days_for_daily_double: u16,
        question_bank: Vec<DailyTriviaQuestionConfig>,
    },
    #[default]
    Simple,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MinigameType {
    Flash {
        game_swf_name: String,
        #[serde(default)]
        game_type: FlashMinigameType,
    },
    SaberStrike {
        saber_strike_stage_id: u32,
    },
}

impl MinigameType {
    pub fn to_type_data(
        &self,
        stage_guid: i32,
        stage_group_guid: i32,
        daily_game_playability: DailyGamePlayability,
    ) -> MinigameTypeData {
        match self {
            MinigameType::Flash { game_type, .. } => match game_type {
                FlashMinigameType::DailySpin { buckets: rewards } => MinigameTypeData::DailySpin {
                    game: Box::new(DailySpinGame::new(
                        rewards,
                        daily_game_playability,
                        stage_guid,
                        stage_group_guid,
                    )),
                },
                FlashMinigameType::DailyHolocron => MinigameTypeData::DailyHolocron {
                    game: Box::new(DailyHolocronGame::new(
                        daily_game_playability,
                        stage_guid,
                        stage_group_guid,
                    )),
                },
                _ => MinigameTypeData::default(),
            },
            MinigameType::SaberStrike { .. } => MinigameTypeData::SaberStrike {
                obfuscated_score: 0,
            },
        }
    }
}

#[non_exhaustive]
#[derive(Clone, Default)]
pub enum MinigameTypeData {
    #[default]
    Empty,
    SaberStrike {
        obfuscated_score: i32,
    },
    DailySpin {
        game: Box<DailySpinGame>,
    },
    DailyHolocron {
        game: Box<DailyHolocronGame>,
    },
    DailyTrivia {
        game: Box<DailyTriviaGame>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchmakingGroupStatus {
    Open,
    Closed,
}

#[derive(Clone, PartialEq, Eq)]
pub enum MinigameReadiness {
    Matchmaking,
    InitialPlayersLoading(BTreeSet<u32>, Option<Instant>),
    Ready(Duration, Option<Instant>),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SharedMinigameDataTickableIndex {
    Tickable,
    NotTickable,
}
pub type SharedMinigameDataMatchmakingIndex = (MatchmakingGroupStatus, i32, Instant);

#[derive(Clone)]
pub struct SharedMinigameData {
    pub guid: MinigameMatchmakingGroup,
    pub readiness: MinigameReadiness,
    pub data: SharedMinigameTypeData,
}

impl SharedMinigameData {
    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        self.data.tick(now)
    }

    pub fn remove_player(
        &mut self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        self.data.remove_player(player, minigame_status)
    }
}

impl
    IndexedGuid<
        MinigameMatchmakingGroup,
        SharedMinigameDataTickableIndex,
        SharedMinigameDataMatchmakingIndex,
    > for SharedMinigameData
{
    fn guid(&self) -> MinigameMatchmakingGroup {
        self.guid
    }

    fn index1(&self) -> SharedMinigameDataTickableIndex {
        match self.data {
            SharedMinigameTypeData::FleetCommander { .. } => {
                SharedMinigameDataTickableIndex::Tickable
            }
            SharedMinigameTypeData::ForceConnection { .. } => {
                SharedMinigameDataTickableIndex::Tickable
            }
            _ => SharedMinigameDataTickableIndex::NotTickable,
        }
    }

    fn index2(&self) -> Option<SharedMinigameDataMatchmakingIndex> {
        let matchmaking_status = match self.readiness {
            MinigameReadiness::Matchmaking => MatchmakingGroupStatus::Open,
            _ => MatchmakingGroupStatus::Closed,
        };

        Some((
            matchmaking_status,
            self.guid.stage_guid,
            self.guid.creation_time,
        ))
    }
}

#[non_exhaustive]
#[derive(Clone, Default)]
pub enum SharedMinigameTypeData {
    FleetCommander {
        game: Box<FleetCommanderGame>,
    },
    ForceConnection {
        game: Box<ForceConnectionGame>,
    },
    #[default]
    None,
}

impl SharedMinigameTypeData {
    pub fn from(
        minigame_type: &MinigameType,
        members: &[u32],
        stage_group_guid: i32,
        stage_guid: i32,
        difficulty: u32,
    ) -> Self {
        // We can't have a game without at least one player
        let player1 = members[0];
        let player2 = members.get(1).cloned();

        match minigame_type {
            MinigameType::Flash { game_type, .. } => match game_type {
                FlashMinigameType::FleetCommander => SharedMinigameTypeData::FleetCommander {
                    game: Box::new(FleetCommanderGame::new(
                        difficulty,
                        player1,
                        player2,
                        stage_guid,
                        stage_group_guid,
                    )),
                },
                FlashMinigameType::ForceConnection => SharedMinigameTypeData::ForceConnection {
                    game: Box::new(ForceConnectionGame::new(
                        player1,
                        player2,
                        stage_guid,
                        stage_group_guid,
                    )),
                },
                _ => SharedMinigameTypeData::default(),
            },
            MinigameType::SaberStrike { .. } => SharedMinigameTypeData::default(),
        }
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        match self {
            SharedMinigameTypeData::FleetCommander { game } => game.tick(now),
            SharedMinigameTypeData::ForceConnection { game } => game.tick(now),
            _ => Vec::new(),
        }
    }

    pub fn remove_player(
        &mut self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        match self {
            SharedMinigameTypeData::FleetCommander { game } => {
                game.remove_player(player, minigame_status)
            }
            SharedMinigameTypeData::ForceConnection { game } => {
                game.remove_player(player, minigame_status)
            }
            _ => Ok(Vec::new()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MinigameTimer {
    paused: bool,
    time_until_next_event: Duration,
    last_timer_update: Instant,
}

impl MinigameTimer {
    pub fn new() -> Self {
        MinigameTimer {
            paused: false,
            time_until_next_event: Duration::ZERO,
            last_timer_update: Instant::now(),
        }
    }

    pub fn new_with_event(duration: Duration) -> Self {
        MinigameTimer {
            paused: false,
            time_until_next_event: duration,
            last_timer_update: Instant::now(),
        }
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn pause_or_resume(&mut self, pause: bool) {
        let now = Instant::now();
        if pause {
            self.time_until_next_event = self.time_until_next_event(now);
        }
        self.last_timer_update = now;
        self.paused = pause;
    }

    pub fn schedule_event(&mut self, duration: Duration) {
        self.last_timer_update = Instant::now();
        self.time_until_next_event = duration;
    }

    pub fn update_timer(&mut self, now: Instant) {
        self.time_until_next_event = self.time_until_next_event(now);
        self.last_timer_update = now;
    }

    pub fn time_until_next_event(&self, now: Instant) -> Duration {
        if self.paused {
            self.time_until_next_event
        } else {
            let time_since_last_tick = now.saturating_duration_since(self.last_timer_update);
            self.time_until_next_event
                .saturating_sub(time_since_last_tick)
        }
    }
}

const CHALLENGE_LINK_NAME: &str = "challenge";
const GROUP_LINK_NAME: &str = "group";

#[derive(Clone, Copy, Deserialize)]
enum DailyGameType {
    Spin,
    Holocron,
    Trivia,
}

impl DailyGameType {
    pub fn key(&self) -> &str {
        match *self {
            DailyGameType::Spin => "Daily Wheel",
            DailyGameType::Holocron => "Daily Holocron",
            DailyGameType::Trivia => "Daily Trivia",
        }
    }

    pub fn boost(&self) -> MinigameBoost {
        match *self {
            DailyGameType::Spin => MinigameBoost::Spin,
            DailyGameType::Holocron => MinigameBoost::Holocron,
            DailyGameType::Trivia => MinigameBoost::Trivia,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum DailyGamePlayability {
    NotYetPlayed {
        boost: MinigameBoost,
        timestamp: DateTime<FixedOffset>,
    },
    OnlyWithBoosts {
        boost: MinigameBoost,
        timestamp: DateTime<FixedOffset>,
    },
    Unplayable {
        timestamp: DateTime<FixedOffset>,
    },
}

impl DailyGamePlayability {
    pub fn time(&self) -> DateTime<FixedOffset> {
        match *self {
            DailyGamePlayability::NotYetPlayed { timestamp, .. } => timestamp,
            DailyGamePlayability::OnlyWithBoosts { timestamp, .. } => timestamp,
            DailyGamePlayability::Unplayable { timestamp, .. } => timestamp,
        }
    }
}

fn has_played_minigame_today(
    now: DateTime<FixedOffset>,
    minigame_stats: &PlayerMinigameStats,
    stage_guid: i32,
) -> bool {
    now.with_time(NaiveTime::MIN)
        .single()
        .and_then(|day_start| {
            minigame_stats
                .last_completion_time(stage_guid)
                .map(|last_completion_time| day_start <= last_completion_time)
        })
        .unwrap_or(false)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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
    pub zone_template_guid: Option<u8>,
    pub score_to_credits_expression: String,
    #[serde(default = "default_matchmaking_timeout_millis")]
    pub matchmaking_timeout_millis: u32,
    pub single_player_stage: Option<StageLocator>,
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
#[serde(deny_unknown_fields)]
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
    #[serde(default = "default_true")]
    pub show_end_score_screen: bool,
    pub required_item_guid: Option<u32>,
    pub members_only: bool,
    #[serde(default = "default_true")]
    pub require_previous_completed: bool,
    pub link_name: String,
    pub short_name: String,
    pub minigame_type: MinigameType,
    pub zone_template_guid: Option<u8>,
    pub score_to_credits_expression: String,
    #[serde(default = "default_matchmaking_timeout_millis")]
    pub matchmaking_timeout_millis: u32,
    pub single_player_stage: Option<StageLocator>,
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
#[serde(deny_unknown_fields)]
enum MinigameStageGroupChild {
    StageGroup(Arc<MinigameStageGroupConfig>),
    Stage(Box<MinigameCampaignStageConfig>),
}

const fn default_true() -> bool {
    true
}

const fn default_matchmaking_timeout_millis() -> u32 {
    10000
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MinigameStageGroupConfig {
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
        minigame_stats: &PlayerMinigameStats,
        now: DateTime<FixedOffset>,
    ) -> (
        Vec<MinigameStageGroupDefinition>,
        Vec<MinigameStageDefinition>,
        bool,
    ) {
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();
        let mut group_links = Vec::new();
        let mut group_played_today = false;

        for (index, child) in self.stages.iter().enumerate() {
            let stage_number = index as u32 + 1;
            match child {
                MinigameStageGroupChild::StageGroup(stage_group) => {
                    let (mut stage_group_definitions, mut stage_definitions, played_today) =
                        stage_group.to_stage_group_definition(
                            portal_entry_guid,
                            minigame_stats,
                            now,
                        );
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

                    group_played_today = group_played_today || played_today;
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
                    group_played_today = group_played_today
                        || has_played_minigame_today(now, minigame_stats, stage.guid);

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
                        group_played_today = group_played_today
                            || has_played_minigame_today(now, minigame_stats, challenge.guid);
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

        (stage_groups, stages, group_played_today)
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
#[serde(deny_unknown_fields)]
struct MinigamePortalEntryConfig {
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
    pub daily_type: Option<DailyGameType>,
    pub sort_order: u32,
    pub tutorial_swf: String,
    pub stage_group: Arc<MinigameStageGroupConfig>,
}

type MinigamePortalEntryDailySettings = (DailyGamePlayability, AddDailyMinigame);
impl MinigamePortalEntryConfig {
    pub fn to_portal_entry(
        &self,
        portal_category_guid: u32,
        minigame_stats: &PlayerMinigameStats,
        now: DateTime<FixedOffset>,
    ) -> (
        MinigamePortalEntry,
        Option<MinigamePortalEntryDailySettings>,
        Vec<MinigameStageGroupDefinition>,
        Vec<MinigameStageDefinition>,
    ) {
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();

        let (mut stage_group_definitions, mut stage_definitions, played_today) = self
            .stage_group
            .to_stage_group_definition(self.guid, minigame_stats, now);
        stage_groups.append(&mut stage_group_definitions);
        stages.append(&mut stage_definitions);

        let daily_settings = self.daily_type.as_ref().map(|daily_type| {
            let daily_game_playability = if !played_today {
                DailyGamePlayability::NotYetPlayed {
                    boost: daily_type.boost(),
                    timestamp: now,
                }
            } else if minigame_stats.boosts_remaining(daily_type.boost()) > 0 {
                DailyGamePlayability::OnlyWithBoosts {
                    boost: daily_type.boost(),
                    timestamp: now,
                }
            } else {
                DailyGamePlayability::Unplayable { timestamp: now }
            };

            let add_packet = AddDailyMinigame {
                initial_state: UpdateDailyMinigame {
                    guid: self.guid,
                    playthroughs_remaining: match daily_game_playability {
                        DailyGamePlayability::Unplayable { .. } => 0,
                        _ => 1,
                    },
                    consecutive_playthroughs_remaining: 0,
                    seconds_until_next_playthrough: 0,
                    seconds_until_reset: 0,
                },
                minigame_name: daily_type.key().to_string(),
                minigame_type: daily_type.key().to_string(),
                multiplier: 1.0,
            };

            (daily_game_playability, add_packet)
        });

        (
            MinigamePortalEntry {
                guid: self.guid,
                name_id: self.name_id,
                description_id: self.description_id,
                members_only: self.members_only,
                is_flash: self.is_flash,
                is_daily_game_locked: daily_settings
                    .as_ref()
                    .map(|(playability, _)| {
                        matches!(playability, DailyGamePlayability::Unplayable { .. })
                    })
                    .unwrap_or(false),
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
            daily_settings,
            stage_groups,
            stages,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MinigamePortalCategoryConfig {
    pub guid: u32,
    pub name_id: u32,
    pub icon_id: u32,
    pub sort_order: u32,
    pub portal_entries: Vec<MinigamePortalEntryConfig>,
}

impl MinigamePortalCategoryConfig {
    pub fn to_definitions(
        &self,
        minigame_stats: &PlayerMinigameStats,
        daily_reset_offset: &DailyResetOffset,
    ) -> (
        MinigamePortalCategory,
        Vec<MinigamePortalEntry>,
        Vec<MinigameStageGroupDefinition>,
        Vec<MinigameStageDefinition>,
        Vec<AddDailyMinigame>,
    ) {
        let mut entries = Vec::new();
        let mut dailies = Vec::new();
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();

        let now = Utc::now().with_timezone(&daily_reset_offset.0);

        for entry in &self.portal_entries {
            let (
                entry_definition,
                possible_daily,
                mut stage_group_definitions,
                mut stage_definitions,
            ) = entry.to_portal_entry(self.guid, minigame_stats, now);
            entries.push(entry_definition);
            if let Some((_, daily)) = possible_daily {
                dailies.push(daily);
            }
            stage_groups.append(&mut stage_group_definitions);
            stages.append(&mut stage_definitions);
        }

        (
            MinigamePortalCategory {
                guid: self.guid,
                name_id: self.name_id,
                icon_id: self.icon_id,
                sort_order: self.sort_order,
            },
            entries,
            stage_groups,
            stages,
            dailies,
        )
    }
}

impl MinigameDefinitions {
    fn from_portal_category_configs(
        value: &[MinigamePortalCategoryConfig],
        minigame_stats: &PlayerMinigameStats,
        daily_reset_offset: &DailyResetOffset,
    ) -> (Self, Vec<AddDailyMinigame>) {
        let mut portal_categories = Vec::new();
        let mut portal_entries = Vec::new();
        let mut stage_groups = Vec::new();
        let mut stages = Vec::new();
        let mut all_dailies = Vec::new();

        for category in value {
            let (
                category_definition,
                mut entry_definitions,
                mut stage_group_definitions,
                mut stage_definitions,
                mut dailies,
            ) = category.to_definitions(minigame_stats, daily_reset_offset);
            portal_categories.push(category_definition);
            portal_entries.append(&mut entry_definitions);
            stage_groups.append(&mut stage_group_definitions);
            stages.append(&mut stage_definitions);
            all_dailies.append(&mut dailies);
        }

        (
            MinigameDefinitions {
                header: MinigameHeader::default(),
                stages,
                stage_groups,
                portal_entries,
                portal_categories,
            },
            all_dailies,
        )
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

    pub fn zone_template_guid(&self) -> Option<u8> {
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

    pub fn single_player_stage(&self) -> Option<StageLocator> {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.single_player_stage,
            MinigameStageConfig::Challenge(challenge, ..) => challenge.single_player_stage,
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

    pub fn show_end_score_screen(&self) -> bool {
        match self {
            MinigameStageConfig::CampaignStage(stage) => stage.show_end_score_screen,
            MinigameStageConfig::Challenge(_, base_stage) => base_stage.show_end_score_screen,
        }
    }
}

pub struct StageConfigRef<'a> {
    pub stage_config: MinigameStageConfig<'a>,
    pub stage_number: u32,
    pub stage_group_guid: i32,
    pub portal_entry_guid: u32,
}

#[derive(Clone)]
pub struct DailyResetOffset(pub FixedOffset);

impl Default for DailyResetOffset {
    fn default() -> Self {
        Self(FixedOffset::east_opt(0).expect("Couldn't create fixed offset of 0"))
    }
}

impl<'de> Deserialize<'de> for DailyResetOffset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let daily_reset_offset_seconds: i32 = Deserialize::deserialize(deserializer)?;

        FixedOffset::east_opt(daily_reset_offset_seconds)
            .map(DailyResetOffset)
            .ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "Daily reset offset {daily_reset_offset_seconds} is longer than a day"
                ))
            })
    }
}

#[derive(Deserialize)]
struct DeserializableMinigameConfigs {
    #[serde(rename(deserialize = "daily_reset_utc_offset_seconds"))]
    daily_reset_offset: DailyResetOffset,
    categories: Vec<MinigamePortalCategoryConfig>,
}

pub struct AllMinigameConfigs {
    pub daily_reset_offset: DailyResetOffset,
    categories: Vec<MinigamePortalCategoryConfig>,
    stage_groups: BTreeMap<i32, (Arc<MinigameStageGroupConfig>, u32)>,
}

impl From<DeserializableMinigameConfigs> for AllMinigameConfigs {
    fn from(value: DeserializableMinigameConfigs) -> Self {
        let mut stage_groups = BTreeMap::new();
        for category in &value.categories {
            for entry in &category.portal_entries {
                insert_stage_groups(entry.guid, &entry.stage_group, &mut stage_groups);
            }
        }

        AllMinigameConfigs {
            daily_reset_offset: value.daily_reset_offset,
            categories: value.categories,
            stage_groups,
        }
    }
}

impl AllMinigameConfigs {
    pub fn seconds_until_minigame_daily_reset(&self) -> u32 {
        86400
            - Utc::now()
                .with_timezone(&self.daily_reset_offset.0)
                .num_seconds_from_midnight()
    }

    pub fn definitions(
        &self,
        minigame_stats: &PlayerMinigameStats,
    ) -> (MinigameDefinitions, Vec<AddDailyMinigame>) {
        MinigameDefinitions::from_portal_category_configs(
            &self.categories[..],
            minigame_stats,
            &self.daily_reset_offset,
        )
    }

    pub fn update_dailies_for_player(
        &self,
        minigame_stats: &PlayerMinigameStats,
    ) -> (Vec<MinigamePortalEntry>, Vec<UpdateDailyMinigame>) {
        let now = Utc::now().with_timezone(&self.daily_reset_offset.0);

        self.categories
            .iter()
            .flat_map(|category| {
                category
                    .portal_entries
                    .iter()
                    .filter_map(|portal_entry_config| {
                        if portal_entry_config.daily_type.is_some() {
                            let (portal_entry, daily, ..) = portal_entry_config.to_portal_entry(
                                category.guid,
                                minigame_stats,
                                now,
                            );

                            daily.map(|(_, add_daily)| (portal_entry, add_daily.initial_state))
                        } else {
                            None
                        }
                    })
            })
            .collect()
    }

    pub fn portal_entry(
        &self,
        portal_entry_guid: u32,
        minigame_stats: &PlayerMinigameStats,
    ) -> Option<(
        MinigamePortalEntry,
        Option<(DailyGamePlayability, AddDailyMinigame)>,
    )> {
        let now = Utc::now().with_timezone(&self.daily_reset_offset.0);

        self.categories.iter().find_map(|category| {
            category
                .portal_entries
                .iter()
                .find_map(|portal_entry_config| {
                    if portal_entry_config.guid == portal_entry_guid {
                        let (portal_entry, daily, ..) =
                            portal_entry_config.to_portal_entry(category.guid, minigame_stats, now);
                        Some((portal_entry, daily))
                    } else {
                        None
                    }
                })
        })
    }

    pub fn stage_group_instance(
        &self,
        stage_group_guid: i32,
        player: &Player,
    ) -> Result<CreateMinigameStageGroupInstance, ProcessPacketError> {
        let Some((stage_group, portal_entry_guid)) = self.stage_groups.get(&stage_group_guid)
        else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Requested unknown stage group instance {stage_group_guid}"),
            ));
        };

        Ok(stage_group.to_stage_group_instance(*portal_entry_guid, player))
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
                        let MinigameStageGroupChild::Stage(stage) = child else {
                            return None;
                        };

                        let stage_number = index as u32 + 1;
                        if stage.guid == stage_guid {
                            return Some(StageConfigRef {
                                stage_config: MinigameStageConfig::CampaignStage(stage),
                                stage_number,
                                stage_group_guid: stage_group.guid,
                                portal_entry_guid: *portal_entry_guid,
                            });
                        }

                        for challenge in &stage.challenges {
                            if challenge.guid == stage_guid {
                                return Some(StageConfigRef {
                                    stage_config: MinigameStageConfig::Challenge(challenge, stage),
                                    stage_number,
                                    stage_group_guid: stage_group.guid,
                                    portal_entry_guid: *portal_entry_guid,
                                });
                            }
                        }

                        None
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

pub fn load_all_minigames(config_dir: &Path) -> Result<AllMinigameConfigs, ConfigError> {
    let mut file = File::open(config_dir.join("minigames.yaml"))?;
    let configs: DeserializableMinigameConfigs = serde_yaml::from_reader(&mut file)?;

    let mut portal_category_guids = BTreeSet::new();
    let mut portal_entry_guids = BTreeSet::new();
    let mut stage_group_guids = BTreeSet::new();
    let mut stage_guids = BTreeSet::new();

    for portal_category in configs.categories.iter() {
        if !portal_category_guids.insert(portal_category.guid) {
            return Err(ConfigError::ConstraintViolated(format!(
                "Two portal categories have GUID {}",
                portal_category.guid
            )));
        }

        for potral_entry in portal_category.portal_entries.iter() {
            if !portal_entry_guids.insert(potral_entry.guid) {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Two portal entries have GUID {}",
                    potral_entry.guid
                )));
            }

            if !stage_group_guids.insert(potral_entry.stage_group.guid) {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Two stage groups have GUID {}",
                    potral_entry.stage_group.guid
                )));
            }

            for child in potral_entry.stage_group.stages.iter() {
                match child {
                    MinigameStageGroupChild::StageGroup(stage_group) => {
                        if !stage_group_guids.insert(stage_group.guid) {
                            return Err(ConfigError::ConstraintViolated(format!(
                                "Two stage groups have GUID {}",
                                stage_group.guid
                            )));
                        }
                    }
                    MinigameStageGroupChild::Stage(stage) => {
                        for stage in iter::once(MinigameStageConfig::CampaignStage(stage)).chain(
                            stage
                                .challenges
                                .iter()
                                .map(|challenge| MinigameStageConfig::Challenge(challenge, stage)),
                        ) {
                            if stage.min_players() == 0 {
                                return Err(ConfigError::ConstraintViolated(format!(
                                    "Stage {} must have a minimum of at least 1 player",
                                    stage.guid()
                                )));
                            }

                            if stage.max_players() < stage.min_players() {
                                return Err(ConfigError::ConstraintViolated(format!(
                                    "Stage {}'s maximum player count is lower than its minimum player count",
                                    stage.guid()
                                )));
                            }

                            if !stage_guids.insert(stage.guid()) {
                                return Err(ConfigError::ConstraintViolated(format!(
                                    "Two stages have GUID {}",
                                    stage.guid()
                                )));
                            }
                        }
                    }
                }
            }
        }
    }

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
                RequestCancelActiveMinigame::deserialize(cursor)?;
                handle_request_cancel_active_minigame(true, sender, game_server)
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
                    format!("Unimplemented minigame op code: {op_code:?} {buffer:x?}"),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown minigame packet: {raw_op_code}, {buffer:x?}"),
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
                let Some(character_read_handle) = characters_read.get(&player_guid(sender)) else {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Unknown character {sender} requested a stage group instance {}",
                            request.header.stage_group_guid
                        ),
                    ));
                };

                let CharacterType::Player(player) = &character_read_handle.stats.character_type
                else {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Non-player character {sender} requested a stage group instance {}",
                            request.header.stage_group_guid
                        ),
                    ));
                };

                Ok(vec![Broadcast::Single(
                    sender,
                    vec![
                        GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: game_server
                                .minigames()
                                .stage_group_instance(request.header.stage_group_guid, player)?,
                        }),
                        GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: ShowStageInstanceSelect {
                                header: MinigameHeader {
                                    stage_guid: -1,
                                    sub_op_code: -1,
                                    stage_group_guid: request.header.stage_group_guid,
                                },
                            },
                        }),
                    ],
                )])
            },
        })
}

fn find_matchmaking_group(
    characters_table_write_handle: &CharacterTableWriteHandle<'_>,
    minigame_data_table_write_handle: &MinigameDataTableWriteHandle<'_>,
    required_space: u32,
    max_players: u32,
    stage_guid: i32,
    start_time: Instant,
) -> Option<(MinigameMatchmakingGroup, u32)> {
    let range = (MatchmakingGroupStatus::Open, stage_guid, start_time)
        ..=(MatchmakingGroupStatus::Open, stage_guid, Instant::now());
    // Iterates from oldest group to newest groups, so groups waiting longer are prioritized first
    let mut group_to_join = None;
    for matchmaking_group in minigame_data_table_write_handle.keys_by_index2_range(range) {
        let players_in_group = characters_table_write_handle
            .keys_by_index4(&matchmaking_group)
            .count() as u32;
        if players_in_group <= max_players.saturating_sub(required_space) {
            group_to_join = Some((
                matchmaking_group,
                max_players.saturating_sub(players_in_group),
            ));
        }
    }

    group_to_join
}

fn set_initial_minigame_status(
    sender: u32,
    group: MinigameMatchmakingGroup,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    stage_config: &StageConfigRef,
) -> Result<(), ProcessPacketError> {
    characters_table_write_handle.update_value_indices(
        player_guid(sender),
        |possible_character_write_handle, _| {
            let Some(character_write_handle) = possible_character_write_handle else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Character {} requested to join a stage {} but does not exist",
                        player_guid(sender),
                        stage_config.stage_config.guid()
                    ),
                ));
            };

            let CharacterType::Player(ref mut player) =
                &mut character_write_handle.stats.character_type
            else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Character {} requested to join a stage {} but is not a player",
                        player_guid(sender),
                        stage_config.stage_config.guid()
                    ),
                ));
            };

            if let Some(minigame_status) = &player.minigame_status {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} requested to join a stage {}, but they are already in minigame group {:?}",
                        stage_config.stage_config.guid(),
                        minigame_status.group
                    ),
                ));
            }

            player.minigame_status = Some(MinigameStatus {
                group,
                teleported_to_game: false,
                game_created: false,
                win_status: MinigameWinStatus::default(),
                score_entries: Vec::new(),
                total_score: 0,
                awarded_credits: 0,
                type_data: MinigameTypeData::Empty,
            });

            Ok(())
        },
    )
}

fn handle_request_create_active_minigame(
    request: RequestCreateActiveMinigame,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let Some(stage_config) = game_server
        .minigames()
        .stage_config(request.header.stage_group_guid, request.header.stage_guid)
    else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {sender} requested to join stage {} in stage group {}, but it doesn't exist",
                request.header.stage_guid, request.header.stage_group_guid
            ),
        ));
    };

    let now = Instant::now();
    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, minigame_data_lock_enforcer| {
            minigame_data_lock_enforcer.write_minigame_data(
                |minigame_data_table_write_handle, zones_lock_enforcer| {
                    zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                        let mut broadcasts = Vec::new();

                        let required_space = 1;
                        let (open_group, space_left) = find_matchmaking_group(
                            characters_table_write_handle,
                            minigame_data_table_write_handle,
                            required_space,
                            stage_config.stage_config.max_players(),
                            stage_config.stage_config.guid(),
                            game_server.start_time(),
                        )
                        .unwrap_or_else(|| {
                            let new_group = MinigameMatchmakingGroup {
                                stage_group_guid: stage_config.stage_group_guid,
                                stage_guid: stage_config.stage_config.guid(),
                                creation_time: now,
                                owner_guid: sender,
                            };

                            (new_group, stage_config.stage_config.max_players())
                        });

                        set_initial_minigame_status(
                            sender,
                            open_group,
                            characters_table_write_handle,
                            &stage_config,
                        )?;

                        // Wait to insert a new group in case there's an error updating the player's status,
                        // and wait to populate the shared type data until all players have joined
                        if minigame_data_table_write_handle.get(open_group).is_none() {
                            minigame_data_table_write_handle.insert(SharedMinigameData {
                                guid: open_group,
                                readiness: MinigameReadiness::Matchmaking,
                                data: SharedMinigameTypeData::None,
                            });
                        }

                        // Start the game because the group is full
                        if space_left <= required_space {
                            let mut players_in_group: Vec<u32> = characters_table_write_handle
                                .keys_by_index4(&open_group)
                                .filter_map(|guid| shorten_player_guid(guid).ok())
                                .collect();
                            players_in_group.shuffle(&mut thread_rng());

                            broadcasts.append(&mut prepare_active_minigame_instance(
                                open_group,
                                &players_in_group,
                                &stage_config,
                                characters_table_write_handle,
                                minigame_data_table_write_handle,
                                zones_table_write_handle,
                                None,
                                game_server,
                            ));
                        } else {
                            broadcasts.push(Broadcast::Single(
                                sender,
                                vec![GamePacket::serialize(&TunneledPacket {
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
                                        unknown7: 0,
                                    },
                                })],
                            ));
                        }

                        Ok(broadcasts)
                    })
                },
            )
        },
    )
}

fn prepare_active_minigame_instance_for_player(
    member_guid: u32,
    new_instance_guid_if_created: Option<u64>,
    stage_config: &StageConfigRef,
    message_id: Option<u32>,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let stage_group_guid = stage_config.stage_group_guid;
    let stage_guid = stage_config.stage_config.guid();

    let mut broadcasts = Vec::new();

    characters_table_write_handle.update_value_indices(player_guid(member_guid), |possible_character_write_handle, _| {
        let Some(character_write_handle) = possible_character_write_handle
        else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Unknown player {member_guid} tried to create an active minigame"
                ),
            ));
        };

        let CharacterType::Player(player) = &mut character_write_handle.stats.character_type
        else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {member_guid} tried to create an active minigame, but their character isn't a player")
            ));
        };

        if !game_server
            .minigames()
            .stage_unlocked(stage_group_guid, stage_guid, player)
        {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {member_guid} tried to create an active minigame for a stage {stage_guid} they haven't unlocked")
            ));
        }

        let (portal_entry, daily_settings) = game_server.minigames()
            .portal_entry(stage_config.portal_entry_guid, &player.minigame_stats)
            .ok_or_else(|| ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Tried to find unknown portal entry {} when creating an active minigame for stage {stage_guid} for player {member_guid}", 
                    stage_config.portal_entry_guid
                )
            ))?;

        if portal_entry.is_daily_game_locked {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {member_guid}'s tried to create an active minigame for a stage {stage_guid}, but it is a daily game that they already played today"
                )
            ));
        }

        let Some(minigame_status) = &mut player.minigame_status else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {member_guid} tried to create an active minigame, but their minigame status is not set")
            ));
        };

        let daily_game_playability = daily_settings.map(|(daily_game_playability, _)| daily_game_playability)
            .unwrap_or(DailyGamePlayability::Unplayable {
                timestamp: Utc::now().with_timezone(&game_server.minigames().daily_reset_offset.0),
            });

        minigame_status.type_data = stage_config.stage_config.minigame_type().to_type_data(
            stage_guid,
            stage_group_guid,
            daily_game_playability,
        );
        minigame_status.group.stage_group_guid = stage_group_guid;
        minigame_status.group.stage_guid = stage_guid;

        Ok(())
    })?;

    if let Some(message) = message_id {
        broadcasts.push(Broadcast::Single(
            member_guid,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SendStringId {
                    sender_guid: player_guid(member_guid),
                    message_id: message,
                    is_anonymous: true,
                    unknown2: false,
                    is_action_bar_message: true,
                    action_bar_text_color: ActionBarTextColor::Yellow,
                    target_guid: 0,
                    owner_guid: 0,
                    unknown7: 0,
                },
            })],
        ));
    }

    if let Some(new_instance_guid) = new_instance_guid_if_created {
        let teleport_result: Result<Vec<Broadcast>, ProcessPacketError> = teleport_to_zone!(
            characters_table_write_handle,
            member_guid,
            zones_table_write_handle,
            &zones_table_write_handle
                .get(new_instance_guid)
                .unwrap_or_else(|| panic!(
                    "Zone instance {new_instance_guid} should have been created or already exist but is missing"
                ))
                .read(),
            None,
            None,
            game_server.mounts(),
        );
        broadcasts.append(&mut teleport_result?);
    }

    // Because we hold the characters table write handle, no one else could have written to the character
    let Some(character) = characters_table_write_handle.get(player_guid(member_guid)) else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Unknown player {member_guid} disappeared after being teleported to a minigame"
            ),
        ));
    };
    let mut character_write_handle = character.write();
    let CharacterType::Player(player) = &mut character_write_handle.stats.character_type else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {member_guid} became a non-player after teleporting them to a minigame"
            ),
        ));
    };

    let Some(minigame_status) = &mut player.minigame_status else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {member_guid} lost their minigame status after being teleported to a minigame"
            ),
        ));
    };
    minigame_status.teleported_to_game = new_instance_guid_if_created.is_some();
    if new_instance_guid_if_created.is_none() {
        broadcasts.append(&mut create_active_minigame_if_uncreated(
            member_guid,
            game_server.minigames(),
            minigame_status,
        )?);
    }

    Ok(broadcasts)
}

pub fn prepare_active_minigame_instance(
    matchmaking_group: MinigameMatchmakingGroup,
    members: &[u32],
    stage_config: &StageConfigRef,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    minigame_data_table_write_handle: &mut MinigameDataTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    message_id: Option<u32>,
    game_server: &GameServer,
) -> Vec<Broadcast> {
    let stage_group_guid = stage_config.stage_group_guid;
    let stage_guid = stage_config.stage_config.guid();

    let mut broadcasts = Vec::new();

    let shared_data_result = minigame_data_table_write_handle.update_value_indices(
        matchmaking_group,
        |possible_minigame_data, _| {
            let Some(minigame_data) = possible_minigame_data else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Unable to find shared minigame data for group {matchmaking_group:?}"),
                ));
            };

            minigame_data.guid.stage_group_guid = stage_group_guid;
            minigame_data.guid.stage_guid = stage_guid;

            minigame_data.data = SharedMinigameTypeData::from(
                stage_config.stage_config.minigame_type(),
                members,
                stage_group_guid,
                stage_guid,
                stage_config.stage_config.difficulty(),
            );

            minigame_data.readiness = MinigameReadiness::InitialPlayersLoading(
                BTreeSet::from_iter(members.iter().copied()),
                None,
            );

            Ok(())
        },
    );

    if let Err(err) = shared_data_result {
        info!("Unable to update shared minigame data: {}", err);
        return broadcasts;
    }

    let teleport_result: Result<Vec<Broadcast>, ProcessPacketError> = (|| {
        let new_instance_guid = match stage_config.stage_config.zone_template_guid() {
            Some(zone_template_guid) => Some(game_server.get_or_create_instance(
                characters_table_write_handle,
                zones_table_write_handle,
                zone_template_guid,
                stage_config.stage_config.max_players(),
            )?),
            None => None,
        };

        let mut teleport_broadcasts = Vec::new();
        for member_guid in members {
            teleport_broadcasts.append(&mut prepare_active_minigame_instance_for_player(
                *member_guid,
                new_instance_guid,
                stage_config,
                message_id,
                characters_table_write_handle,
                zones_table_write_handle,
                game_server,
            )?);
        }

        Ok(teleport_broadcasts)
    })();

    match teleport_result {
        Ok(mut teleport_broadcasts) => broadcasts.append(&mut teleport_broadcasts),
        Err(err) => {
            // Teleportation out of the minigame zone should clean it up if it is empty. If there is some error, the next game that starts
            // can use the zone
            info!("Couldn't add a player to the minigame, ending the game: {} (stage group {}, stage {})", err, stage_group_guid, stage_guid);
            let leave_result = leave_active_minigame_if_any(
                LeaveMinigameTarget::Group(matchmaking_group),
                characters_table_write_handle,
                minigame_data_table_write_handle,
                zones_table_write_handle,
                message_id,
                false,
                game_server,
            );

            match leave_result {
                Ok(mut leave_broadcasts) => broadcasts.append(&mut leave_broadcasts),
                Err(err) => info!("Unable to remove players after trying to prepare the active minigame for group {:?}: {}", matchmaking_group, err),
            }
        }
    }

    // Don't return a result here so that we properly handle updates for all players in the group, rather than returning early
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
        character_consumer: |_, characters_read, _, minigame_data_lock_enforcer| {
            let Some(character_read_handle) = characters_read.get(&player_guid(sender)) else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Unknown player {sender} requested to start an active minigame")
                ));
            };

            let CharacterType::Player(player) = &character_read_handle.stats.character_type else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Player {sender} requested to start an active minigame, but their character isn't a player")
                ));
            };

            let Some(minigame_status) = &player.minigame_status else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} requested to start an active minigame (stage {}), but they aren't in an active minigame",
                        request.header.stage_guid
                    )
                ));
            };

            if request.header.stage_guid != minigame_status.group.stage_guid {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} requested to start an active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})",
                        request.header.stage_guid,
                        minigame_status.group.stage_group_guid,
                        minigame_status.group.stage_guid
                    )
                ));
            };

            let mut packets = Vec::new();

            let Some(StageConfigRef {stage_config, ..}) = game_server.minigames().stage_config(minigame_status.group.stage_group_guid, minigame_status.group.stage_guid) else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} requested to start active minigame with stage config {} (stage group {}) that does not exist",
                        minigame_status.group.stage_guid,
                        minigame_status.group.stage_group_guid
                    )
                ));
            };

            minigame_data_lock_enforcer.read_minigame_data(|_| MinigameDataLockRequest {
                read_guids: Vec::new(),
                write_guids: vec![minigame_status.group],
                minigame_data_consumer: |_, _, mut minigame_data_write, _| {
                    let Some(shared_minigame_data) = minigame_data_write.get_mut(&minigame_status.group) else {
                        return Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Unable to find shared minigame data for group {:?} when starting minigame for player {sender}",
                                minigame_status.group
                            )
                        ));
                    };

                    if let MinigameReadiness::InitialPlayersLoading(players, first_player_load_time) = &mut shared_minigame_data.readiness {
                        let now = Instant::now();
                        if first_player_load_time.is_none() {
                            *first_player_load_time = Some(now);
                        }

                        if players.remove(&sender) && players.is_empty() {
                            shared_minigame_data.readiness = MinigameReadiness::Ready(Duration::ZERO, Some(first_player_load_time.unwrap_or(now)));
                        }
                    }

                    let mut stage_group_instance =
                    game_server.minigames().stage_group_instance(minigame_status.group.stage_group_guid, player)?;
                    stage_group_instance.header.stage_guid = minigame_status.group.stage_guid;
                    // The default stage instance must be set for the how-to button the options menu to work
                    stage_group_instance.default_stage_instance_guid = minigame_status.group.stage_guid;

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
                    }));
                    packets.push(GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: stage_group_instance,
                    }));

                    match stage_config.minigame_type() {
                        MinigameType::Flash { game_swf_name, .. } => {
                            packets.push(
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: StartFlashGame {
                                        loader_script_name: "MiniGameFlash".to_string(),
                                        game_swf_name: game_swf_name.clone(),
                                        return_to_portal: !stage_config.show_end_score_screen(),
                                    },
                                })
                            );
                        },
                        MinigameType::SaberStrike { saber_strike_stage_id } => {
                            packets.push(
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: SaberStrikeStageData {
                                        minigame_header: MinigameHeader {
                                            stage_guid: minigame_status.group.stage_guid,
                                            sub_op_code: SaberStrikeOpCode::StageData as i32,
                                            stage_group_guid: minigame_status.group.stage_group_guid,
                                        },
                                        saber_strike_stage_id: *saber_strike_stage_id,
                                        use_player_weapon: player.battle_classes.get(&player.active_battle_class)
                                            .and_then(|battle_class| battle_class.items.get(&EquipmentSlot::PrimaryWeapon)
                                            .and_then(|item| game_server.items().get(&item.guid)))
                                            .map(|item| item.item_type == SABER_ITEM_TYPE)
                                            .unwrap_or(false),
                                    }
                                }),
                            );
                        },
                    }

                    packets.push(
                        GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: StartActiveMinigame {
                                header: MinigameHeader {
                                    stage_guid: minigame_status.group.stage_guid,
                                    sub_op_code: -1,
                                    stage_group_guid: minigame_status.group.stage_group_guid,
                                },
                            },
                        }),
                    );

                    Ok(vec![
                        Broadcast::Single(sender, packets)
                    ])
                },
            })
        }
    })
}

fn handle_request_cancel_active_minigame(
    skip_if_flash: bool,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, minigame_data_lock_enforcer| {
            minigame_data_lock_enforcer.write_minigame_data(
                |minigame_data_table_write_handle, zones_lock_enforcer| {
                    zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                        leave_active_minigame_if_any(
                            LeaveMinigameTarget::Single(sender),
                            characters_table_write_handle,
                            minigame_data_table_write_handle,
                            zones_table_write_handle,
                            None,
                            skip_if_flash,
                            game_server,
                        )
                    })
                },
            )
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
        &mut SharedMinigameData,
        &CharacterTableReadHandle,
    ) -> Result<T, ProcessPacketError>,
) -> Result<T, ProcessPacketError> {
    game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
        read_guids: Vec::new(),
        write_guids: vec![player_guid(sender)],
        character_consumer: |characters_table_read_handle, _, mut characters_write, minigame_data_lock_enforcer|  {
            let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Tried to process packet for unknown player {sender}'s active minigame"),
                ));
            };

            let CharacterType::Player(player) = &mut character_write_handle.stats.character_type else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process packet for {sender}'s active minigame, but their character isn't a player"
                    ),
                ));
            };

            let Some(minigame_status) = &mut player.minigame_status else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process packet for {sender}'s active minigame (stage {}), but they aren't in an active minigame",
                        header.stage_guid
                    )
                ));
            };

            if header.stage_guid != minigame_status.group.stage_guid {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process packet for {sender}'s active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})",
                        header.stage_guid,
                        minigame_status.group.stage_group_guid,
                        minigame_status.group.stage_guid
                    )
                ));
            };

            let Some(stage_config_ref) = game_server
                .minigames()
                .stage_config(minigame_status.group.stage_group_guid, minigame_status.group.stage_guid) else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process packet for {sender}'s active minigame with stage config {} (stage group {}) that does not exist",
                        minigame_status.group.stage_guid,
                        minigame_status.group.stage_group_guid
                    )
                ));
            };

            minigame_data_lock_enforcer.read_minigame_data(|_| MinigameDataLockRequest {
                read_guids: Vec::new(),
                write_guids: vec![minigame_status.group],
                minigame_data_consumer: |_, _, mut minigame_data_write, _| {
                    let Some(shared_minigame_data) = minigame_data_write.get_mut(&minigame_status.group) else {
                        return Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Tried to process packet for {sender}'s active minigame with stage config {} (stage group {}) that is missing shared minigame data",
                                minigame_status.group.stage_guid,
                                minigame_status.group.stage_group_guid
                            )
                        ));
                    };

                    func(minigame_status, &mut player.minigame_stats, &mut player.credits, stage_config_ref, shared_minigame_data, characters_table_read_handle)
                },
            })
        },
    })
}

fn handle_flash_payload_read_only<T: Default>(
    sender: u32,
    game_server: &GameServer,
    header: &MinigameHeader,
    func: impl FnOnce(
        &MinigameStatus,
        StageConfigRef,
        &SharedMinigameData,
    ) -> Result<T, ProcessPacketError>,
) -> Result<T, ProcessPacketError> {
    game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
        read_guids: vec![player_guid(sender)],
        write_guids: Vec::new(),
        character_consumer: |_, characters_read, _, minigame_data_lock_enforcer|  {
            let Some(character_read_handle) = characters_read.get(&player_guid(sender)) else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Tried to process Flash payload for unknown player {sender}'s active minigame"),
                ));
            };

            let CharacterType::Player(player) = &character_read_handle.stats.character_type else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process Flash payload for {sender}'s active minigame, but their character isn't a player"
                    ),
                ));
            };

            let Some(minigame_status) = &player.minigame_status else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process Flash payload for {sender}'s active minigame (stage {}), but they aren't in an active minigame",
                        header.stage_guid
                    )
                ));
            };

            if header.stage_guid != minigame_status.group.stage_guid {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process Flash payload for {sender}'s active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})",
                        header.stage_guid,
                        minigame_status.group.stage_group_guid,
                        minigame_status.group.stage_guid
                    )
                ));
            }

            let Some(stage_config_ref) = game_server
                .minigames()
                .stage_config(minigame_status.group.stage_group_guid, minigame_status.group.stage_guid) else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to process Flash payload for {sender}'s active minigame with stage config {} (stage group {}) that does not exist",
                        minigame_status.group.stage_guid,
                        minigame_status.group.stage_group_guid
                    )
                ));
            };

            minigame_data_lock_enforcer.read_minigame_data(|_| MinigameDataLockRequest {
                read_guids: vec![minigame_status.group],
                write_guids: Vec::new(),
                minigame_data_consumer: |_, minigame_data_read, _, _| {
                    let Some(shared_minigame_data) = minigame_data_read.get(&minigame_status.group) else {
                        return Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Tried to process Flash payload for {sender}'s active minigame with stage config {} (stage group {}) that is missing shared minigame data",
                                minigame_status.group.stage_guid,
                                minigame_status.group.stage_group_guid
                            )
                        ));
                    };

                    func(minigame_status, stage_config_ref, shared_minigame_data)
                },
            })
        },
    })
}

fn handle_flash_payload_game_result(
    parts: &[&str],
    won: bool,
    sender: u32,
    payload: &FlashPayload,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    handle_minigame_packet_write(
        sender,
        game_server,
        &payload.header,
        |minigame_status, _, _, _, shared_minigame_data, _| {
            if parts.len() != 2 {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Expected 1 parameter in game won payload, but only found {}",
                        parts.len().saturating_sub(1)
                    ),
                ));
            }

            let MinigameReadiness::Ready(previous_time, possible_resume_time) =
                shared_minigame_data.readiness
            else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Player {sender} sent a Flash game result payload (won: {won}) for a game that isn't ready yet")
                ));
            };

            let now = Instant::now();
            let total_time = previous_time
                .saturating_add(now.saturating_duration_since(possible_resume_time.unwrap_or(now)))
                .as_millis();

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
            minigame_status.win_status.set_won(won);

            Ok(vec![Broadcast::Single(
                sender,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: minigame_status.group.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.group.stage_group_guid,
                        },
                        payload: format!("OnGamePlayTimeMsg\t{total_time}"),
                    },
                })],
            )])
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

    let result = match parts[0] {
        "FRServer_RequestStageId" => handle_flash_payload_read_only(
            sender,
            game_server,
            &payload.header,
            |minigame_status, stage_config_ref, _| {
                Ok(vec![Broadcast::Single(
                    sender,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: FlashPayload {
                            header: MinigameHeader {
                                stage_guid: minigame_status.group.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.group.stage_group_guid,
                            },
                            payload: format!(
                                "VOnServerSetStageIdMsg\t{}",
                                stage_config_ref.stage_number
                            ),
                        },
                    })],
                )])
            },
        ),
        "FRServer_ScoreInfo" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, _, _, _, _| {
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
            |minigame_status, _, player_credits, stage_config, _, _| {
                if parts.len() == 2 {
                    let round_score = parts[1].parse()?;
                    let (mut broadcasts, awarded_credits) = award_credits(
                        sender,
                        player_credits,
                        &mut minigame_status.awarded_credits,
                        &stage_config.stage_config,
                        round_score,
                    )?;

                    broadcasts.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: FlashPayload {
                                header: MinigameHeader {
                                    stage_guid: minigame_status.group.stage_guid,
                                    sub_op_code: -1,
                                    stage_group_guid: minigame_status.group.stage_group_guid,
                                },
                                payload: format!("OnShowEndRoundScreenMsg\t{awarded_credits}"),
                            },
                        })],
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
        "FRServer_GameWon" => {
            handle_flash_payload_game_result(&parts, true, sender, &payload, game_server)
        }
        "FRServer_GameLost" => {
            handle_flash_payload_game_result(&parts, false, sender, &payload, game_server)
        }
        "FRServer_GameClose" => handle_request_cancel_active_minigame(false, sender, game_server),
        "FRServer_StatUpdate" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, minigame_stats, _, _, _, _| {
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
        "FRServer_Pause" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, stage_config, shared_minigame_data, _| {
                if parts.len() == 2 {
                    let pause = parts[1].parse()?;

                    if stage_config.stage_config.max_players() == 1 {
                        if let MinigameReadiness::Ready(previous_time, possible_resume_time) =
                            &mut shared_minigame_data.readiness
                        {
                            let now = Instant::now();
                            if pause {
                                if let Some(resume_time) = possible_resume_time {
                                    *previous_time = previous_time.saturating_add(
                                        now.saturating_duration_since(*resume_time),
                                    );
                                    *possible_resume_time = None;
                                }
                            } else {
                                *possible_resume_time = Some(now);
                            }
                        }
                    }

                    match &mut shared_minigame_data.data {
                        SharedMinigameTypeData::FleetCommander { game } => {
                            game.pause_or_resume(sender, pause)
                        }
                        SharedMinigameTypeData::ForceConnection { game } => {
                            game.pause_or_resume(sender, pause)
                        }
                        _ => Ok(Vec::new()),
                    }
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Expected 1 parameter in pause payload, but only found {}",
                            parts.len().saturating_sub(1)
                        ),
                    ))
                }
            },
        ),
        "OnConnectMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, minigame_stats, _, _, shared_minigame_data, characters_table_read_handle| {
                match &mut shared_minigame_data.data {
                    SharedMinigameTypeData::FleetCommander { game } => {
                        game.connect(sender, characters_table_read_handle)
                    }
                    SharedMinigameTypeData::ForceConnection { game } => {
                        game.connect(sender, characters_table_read_handle)
                    }
                    _ => match &mut minigame_status.type_data {
                        MinigameTypeData::DailySpin { game } => game.connect(sender, minigame_stats),
                        MinigameTypeData::DailyHolocron { game } => game.connect(
                            sender,
                            minigame_stats,
                            &game_server.minigames().daily_reset_offset
                        ),
                        _ => Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Received connect message for unexpected game from player {sender}"
                            ),
                        ))
                    },
                }
            },
        ),
        "OnPlayerReadyMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::ForceConnection { game } => game.mark_player_ready(sender),
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received player ready message for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnSelectNewColumnMsg" => handle_flash_payload_read_only(
            sender,
            game_server,
            &payload.header,
            |_, _, shared_minigame_data| match &shared_minigame_data.data {
                SharedMinigameTypeData::ForceConnection { game } => {
                    if parts.len() == 3 {
                        let col = parts[1].parse()?;
                        let player_index = parts[2].parse()?;
                        game.select_column(sender, col, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 2 parameters in select column payload, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received select column message for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnRequestDropPieceMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::ForceConnection { game } => {
                    if parts.len() == 3 {
                        let col = parts[1].parse()?;
                        let player_index = parts[2].parse()?;
                        game.drop_piece(sender, col, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 2 parameters in drop piece payload, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received drop piece message for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnTogglePowerUpMsg" => handle_flash_payload_read_only(
            sender,
            game_server,
            &payload.header,
            |_, _, shared_minigame_data| match &shared_minigame_data.data {
                SharedMinigameTypeData::ForceConnection { game } => {
                    if parts.len() == 3 {
                        let powerup = parts[1].parse()?;
                        let player_index = parts[2].parse()?;
                        game.toggle_powerup(sender, powerup, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!(
                                    "Expected 2 parameters in toggle powerup payload, but only found {}",
                                    parts.len().saturating_sub(1)
                                ),
                            ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received toggle powerup message for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnRequestUseLightPowerUpMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::ForceConnection { game } => {
                    if parts.len() == 6 {
                        let row1 = parts[1].parse()?;
                        let col1 = parts[2].parse()?;
                        let row2 = parts[3].parse()?;
                        let col2 = parts[4].parse()?;
                        let player_index = parts[5].parse()?;
                        game.use_swap_powerup(sender, row1, col1, row2, col2, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 5 parameters in swap piece payload, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received swap powerup request for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnRequestUseDarkPowerUpMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::ForceConnection { game } => {
                    if parts.len() == 4 {
                        let row = parts[1].parse()?;
                        let col = parts[2].parse()?;
                        let player_index = parts[3].parse()?;
                        game.use_delete_powerup(sender, row, col, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 3 parameters in delete piece payload, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received delete powerup request for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnRequestPlaceShipMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::FleetCommander { game } => {
                    if parts.len() == 6 {
                        let ship_size = parts[1].parse()?;
                        let flipped = parts[2].parse::<u8>()? == 1;
                        let row = parts[3].parse()?;
                        let col = parts[4].parse()?;
                        let player_index = parts[5].parse()?;
                        game.place_ship(sender, ship_size, flipped, row, col, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 5 parameters in place ship request, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received place ship request for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnRequestBombGridMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::FleetCommander { game } => {
                    if parts.len() == 4 {
                        let row = parts[1].parse()?;
                        let col = parts[2].parse()?;
                        let player_index = parts[3].parse()?;
                        game.hit(sender, row, col, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 3 parameters in bomb grid request, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received bomb grid request for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnRequestUsePowerUpMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::FleetCommander { game } => {
                    if parts.len() == 5 {
                        let powerup = parts[1].parse()?;
                        let row = parts[2].parse()?;
                        let col = parts[3].parse()?;
                        let attacker_index = parts[4].parse()?;
                        game.use_powerup(sender, row, col, attacker_index, powerup)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 4 parameters in use powerup request, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received use powerup request for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnFinishedPowerUpAnimMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |_, _, _, _, shared_minigame_data, _| match &mut shared_minigame_data.data {
                SharedMinigameTypeData::FleetCommander { game } => {
                    if parts.len() == 2 {
                        let player_index = parts[1].parse()?;
                        game.complete_powerup_animation(sender, player_index)
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Expected 1 parameter in complete powerup animation payload, but only found {}",
                                parts.len().saturating_sub(1)
                            ),
                        ))
                    }
                }
                _ => Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Received complete powerup animation payload for unexpected game from player {sender}"
                    ),
                )),
            },
        ),
        "OnStartGameMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, _, _, _, _| {
                 match &mut minigame_status.type_data {
                    MinigameTypeData::DailySpin { game } => game.mark_player_ready(sender),
                    _ => Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Received start game request for unexpected game from player {sender}"
                        ),
                    ))
                }
            }
        ),
        "OnWheelSpinRequestMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, minigame_stats, _, _, _, _| {
                 match &mut minigame_status.type_data {
                    MinigameTypeData::DailySpin { game } => game.spin(
                        sender,
                        &mut minigame_status.total_score,
                        &mut minigame_status.win_status,
                        &mut minigame_status.score_entries,
                        minigame_stats
                    ),
                    _ => Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Received spin request for unexpected game from player {sender}"
                        ),
                    ))
                }
            }
        ),
        "OnWheelStopMsg" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, player_credits, stage_config, _, _| {
                 match &mut minigame_status.type_data {
                    MinigameTypeData::DailySpin { game } => game.stop_spin(
                        sender,
                        player_credits,
                        &mut minigame_status.awarded_credits,
                        &stage_config.stage_config
                    ),
                    _ => Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Received stop spin request for unexpected game from player {sender}"
                        ),
                    ))
                }
            }
        ),
        "OnChangeWheelRequestMsg" => Ok(Vec::new()),
        "OnPickHolocronRequest" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, _, _, _, _| {
                if parts.len() == 2 {
                    let _: u8 = parts[1].parse()?;
                    match &mut minigame_status.type_data {
                        MinigameTypeData::DailyHolocron { game } => game.select_holocron(
                            sender,
                            &mut minigame_status.total_score,
                            &mut minigame_status.win_status,
                            &mut minigame_status.score_entries,
                        ),
                        _ => Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Received pick holocron request for unexpected game from player {sender}"
                            ),
                        ))
                    }
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Expected 1 parameter in pick holocron request payload, but only found {}",
                            parts.len().saturating_sub(1)
                        ),
                    ))
                }
            }
        ),
        "OnRewardRequest" => handle_minigame_packet_write(
            sender,
            game_server,
            &payload.header,
            |minigame_status, _, player_credits, stage_config, _, _| {
                 match &mut minigame_status.type_data {
                    MinigameTypeData::DailyHolocron { game } => game.display_reward(
                        sender,
                        player_credits,
                        &mut minigame_status.awarded_credits,
                        &stage_config.stage_config
                    ),
                    _ => Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Received reward request for unexpected game from player {sender}"
                        ),
                    ))
                }
            }
        ),
        _ => Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Received unknown Flash payload {} in stage {}, stage group {} from player {sender}",
                payload.payload, payload.header.stage_guid, payload.header.stage_group_guid
            ),
        )),
    };

    result.map_err(|err| {
        err.wrap(format!(
            "Error while processing Flash payload \"{}\" from player {sender}",
            payload.payload
        ))
    })
}

pub fn create_active_minigame_if_uncreated(
    sender: u32,
    minigames: &AllMinigameConfigs,
    minigame_status: &mut MinigameStatus,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let Some(StageConfigRef {
        stage_config,
        portal_entry_guid,
        ..
    }) = minigames.stage_config(
        minigame_status.group.stage_group_guid,
        minigame_status.group.stage_guid,
    )
    else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {sender} requested creation of unknown stage {} (stage group {})",
                minigame_status.group.stage_guid, minigame_status.group.stage_group_guid
            ),
        ));
    };

    if minigame_status.game_created {
        return Ok(Vec::new());
    }
    minigame_status.game_created = true;

    Ok(vec![Broadcast::Single(
        sender,
        vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: ActiveMinigameCreationResult {
                    header: MinigameHeader {
                        stage_guid: minigame_status.group.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: minigame_status.group.stage_group_guid,
                    },
                    was_successful: true,
                },
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: CreateActiveMinigame {
                    header: MinigameHeader {
                        stage_guid: minigame_status.group.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: minigame_status.group.stage_group_guid,
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
                    show_end_score_screen: stage_config.show_end_score_screen(),
                    unknown18: "".to_string(),
                    unknown19: 0,
                    unknown20: false,
                    stage_definition_guid: minigame_status.group.stage_guid,
                    unknown22: false,
                    unknown23: false,
                    unknown24: false,
                    unknown25: 0,
                    unknown26: 0,
                    unknown27: 0,
                },
            }),
        ],
    )])
}

pub fn award_credits(
    sender: u32,
    player_credits: &mut u32,
    game_awarded_credits: &mut u32,
    stage_config: &MinigameStageConfig,
    score: i32,
) -> Result<(Vec<Broadcast>, u32), ProcessPacketError> {
    let awarded_credits =
        evaluate_score_to_credits_expression(stage_config.score_to_credits_expression(), score)?
            .max(0) as u32;

    *game_awarded_credits = game_awarded_credits.saturating_add(awarded_credits);

    let new_credits = player_credits.saturating_add(awarded_credits);
    *player_credits = new_credits;

    let broadcasts = vec![Broadcast::Single(
        sender,
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdateCredits { new_credits },
        })],
    )];

    Ok((broadcasts, awarded_credits))
}

enum MinigameRemovalMode {
    Skip,
    NoTeleport,
    Teleport(PreviousLocation),
}

fn leave_active_minigame_single_player_if_any(
    sender: u32,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    shared_minigame_data: &mut SharedMinigameData,
    stage_config: &StageConfigRef<'_>,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let status_update_result = characters_table_write_handle
        .update_value_indices(player_guid(sender), |possible_character_write_handle, _| {
            let Some(character_write_handle) = possible_character_write_handle else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Tried to end unknown player {sender}'s active minigame"),
                ));
            };

            let CharacterType::Player(player) = &mut character_write_handle.stats.character_type
            else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                    "Tried to end player {sender}'s active minigame, but their character isn't a player"
                ),
                ));
            };

            let Some(minigame_status) = &mut player.minigame_status else {
                return Ok(None);
            };

            let removal_mode = match minigame_status.teleported_to_game {
                true => MinigameRemovalMode::Teleport(player.previous_location.clone()),
                false => MinigameRemovalMode::NoTeleport,
            };

            let mut broadcasts = Vec::new();

            broadcasts.append(
                &mut shared_minigame_data
                    .remove_player(sender, minigame_status)?,
            );

            // If we've already awarded credits after a round, don't grant those credits again
            if minigame_status.awarded_credits == 0 {
                broadcasts.append(
                    &mut award_credits(
                        sender,
                        &mut player.credits,
                        &mut minigame_status.awarded_credits,
                        &stage_config.stage_config,
                        minigame_status.total_score,
                    )?
                    .0,
                );
            }

            if let Some(win_time) = minigame_status.win_status.0 {
                player.minigame_stats.complete(
                    minigame_status.group.stage_guid,
                    minigame_status.total_score,
                    win_time,
                    &game_server.minigames().daily_reset_offset
                );
            }

            let last_broadcast = Broadcast::Single(
                sender,
                vec![
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: ExecuteScriptWithStringParams {
                            script_name: "MiniGameFlash.StopAllSounds".to_string(),
                            params: vec![],
                        },
                    }),
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: FlashPayload {
                            header: MinigameHeader {
                                stage_guid: minigame_status.group.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.group.stage_group_guid,
                            },
                            payload: if minigame_status.win_status.won() {
                                "OnGameWonMsg".to_string()
                            } else {
                                "OnGameLostMsg".to_string()
                            },
                        },
                    }),
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: ActiveMinigameEndScore {
                            header: MinigameHeader {
                                stage_guid: minigame_status.group.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.group.stage_group_guid,
                            },
                            scores: minigame_status.score_entries.clone(),
                            unknown2: true,
                        },
                    }),
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: UpdateActiveMinigameRewards {
                            header: MinigameHeader {
                                stage_guid: minigame_status.group.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.group.stage_group_guid,
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
                    }),
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: EndActiveMinigame {
                            header: MinigameHeader {
                                stage_guid: minigame_status.group.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.group.stage_group_guid,
                            },
                            won: minigame_status.win_status.won(),
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: 0,
                        },
                    }),
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: LeaveActiveMinigame {
                            header: MinigameHeader {
                                stage_guid: minigame_status.group.stage_guid,
                                sub_op_code: -1,
                                stage_group_guid: minigame_status.group.stage_group_guid,
                            },
                        },
                    }),
                ],
            );

            let (portal_entry, possible_daily) = game_server.minigames()
                .portal_entry(stage_config.portal_entry_guid, &player.minigame_stats)
                .ok_or_else(|| ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Tried to find unknown portal entry {} when exiting stage {} for player {sender}", 
                        stage_config.portal_entry_guid,
                        stage_config.stage_config.guid()
                    )
                ))?;

            broadcasts.push(Broadcast::Single(sender, vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: MinigameDefinitionsUpdate {
                        definitions: MinigameDefinitions {
                            header: MinigameHeader::default(),
                            stages: Vec::new(),
                            stage_groups: Vec::new(),
                            portal_entries: vec![portal_entry],
                            portal_categories: Vec::new()
                        }
                    },
                }),
            ]));

            if let Some((_, daily)) = possible_daily {
                broadcasts.push(Broadcast::Single(sender, vec![
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: daily.initial_state,
                    }),
                ]));
            }

            broadcasts.push(Broadcast::Single(
                sender,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: game_server
                        .minigames()
                        .stage_group_instance(minigame_status.group.stage_group_guid, player)?,
                })],
            ));
            broadcasts.push(last_broadcast);

            player.minigame_status = None;

            Ok(Some((broadcasts, removal_mode)))
        })?;

    let Some((mut broadcasts, removal_move)) = status_update_result else {
        return Ok(Vec::new());
    };

    let MinigameRemovalMode::Teleport(previous_location) = removal_move else {
        return Ok(broadcasts);
    };

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
                "Zone instance {instance_guid} should have been created or already exist but is missing"
            ))
            .read(),
        Some(previous_location.pos),
        Some(previous_location.rot),
        game_server.mounts(),
    );
    broadcasts.append(&mut teleport_broadcasts?);

    Ok(broadcasts)
}

fn remove_single_player_from_matchmaking(
    player: u32,
    stage_group_guid: i32,
    stage_guid: i32,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    message_id: Option<u32>,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let removal_mode = characters_table_write_handle.update_value_indices(player_guid(player), |possible_character_write_handle, _| {
        let Some(character_write_handle) = possible_character_write_handle else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Tried to remove unknown player {player} from matchmaking"),
            ));
        };

        let CharacterType::Player(player) = &mut character_write_handle.stats.character_type else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Tried to remove player {player} from matchmaking, but their character isn't a player"
                ),
            ));
        };

        let Some(minigame_status) = &player.minigame_status else {
            return Ok(MinigameRemovalMode::Skip);
        };

        let mode = if minigame_status.teleported_to_game {
            MinigameRemovalMode::Teleport(player.previous_location.clone())
        } else {
            MinigameRemovalMode::NoTeleport
        };

        player.minigame_status = None;

        Ok(mode)
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
    })];
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
        }));
    }
    broadcasts.push(Broadcast::Single(player, result_packets));

    if let MinigameRemovalMode::Teleport(previous_location) = removal_mode {
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
                    "Zone instance {instance_guid} should have been created or already exist but is missing"
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

fn leave_or_remove_single_player_from_matchmaking(
    sender: u32,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    shared_minigame_data: &mut SharedMinigameData,
    stage_group_guid: i32,
    stage_config: &StageConfigRef<'_>,
    is_matchmaking: bool,
    matchmaking_removal_message_id: Option<u32>,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    if is_matchmaking {
        remove_single_player_from_matchmaking(
            sender,
            stage_group_guid,
            stage_config.stage_config.guid(),
            characters_table_write_handle,
            zones_table_write_handle,
            matchmaking_removal_message_id,
            game_server,
        )
    } else {
        leave_active_minigame_single_player_if_any(
            sender,
            characters_table_write_handle,
            zones_table_write_handle,
            shared_minigame_data,
            stage_config,
            game_server,
        )
    }
}

#[derive(Debug)]
pub enum LeaveMinigameTarget {
    Single(u32),
    Group(MinigameMatchmakingGroup),
}

pub fn leave_active_minigame_if_any(
    target: LeaveMinigameTarget,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    minigame_data_table_write_handle: &mut MinigameDataTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    matchmaking_removal_message_id: Option<u32>,
    skip_if_flash: bool,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let group = match target {
        LeaveMinigameTarget::Single(sender) => {
            let Some(group) = characters_table_write_handle
                .index4(player_guid(sender))
                .cloned()
            else {
                return Ok(Vec::new());
            };

            group
        }
        LeaveMinigameTarget::Group(group) => group,
    };

    let Some(stage_config) = game_server
        .minigames()
        .stage_config(group.stage_group_guid, group.stage_guid)
    else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Tried to end target {target:?}'s active minigame with stage config {} (stage group {}) that does not exist",
                group.stage_guid,
                group.stage_group_guid
            )
        ));
    };

    let Some(shared_minigame_data) = minigame_data_table_write_handle.get(group) else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Tried to end target {target:?}'s active minigame with shared minigame data {} (stage group {}) that does not exist",
                group.stage_guid,
                group.stage_group_guid
            )
        ));
    };
    let mut minigame_data_write_handle = shared_minigame_data.write();
    let is_matchmaking = minigame_data_write_handle.readiness == MinigameReadiness::Matchmaking;

    // Wait for the end signal from the Flash payload because those games send additional score data
    if skip_if_flash
        && matches!(
            stage_config.stage_config.minigame_type(),
            MinigameType::Flash { .. }
        )
    {
        return Ok(Vec::new());
    }

    let mut broadcasts = Vec::new();

    if let LeaveMinigameTarget::Single(sender) = target {
        let leave_result = leave_or_remove_single_player_from_matchmaking(
            sender,
            characters_table_write_handle,
            zones_table_write_handle,
            &mut minigame_data_write_handle,
            group.stage_group_guid,
            &stage_config,
            is_matchmaking,
            matchmaking_removal_message_id,
            game_server,
        );
        match leave_result {
            Ok(mut leave_broadcasts) => broadcasts.append(&mut leave_broadcasts),
            Err(err) => info!(
                "Unable to remove player {} from minigame (stage group {}, stage {}): {}",
                sender, group.stage_group_guid, group.stage_guid, err
            ),
        }
    }

    let remaining_players = characters_table_write_handle.keys_by_index4(&group).count() as u32;
    let is_group_removal = matches!(target, LeaveMinigameTarget::Group { .. });

    if (remaining_players < stage_config.stage_config.min_players() && !is_matchmaking)
        || is_group_removal
    {
        let member_guids: Vec<u64> = characters_table_write_handle
            .keys_by_index4(&group)
            .collect();
        for member_guid in member_guids {
            let other_player_leave_result =
                shorten_player_guid(member_guid).and_then(|short_member_guid| {
                    leave_or_remove_single_player_from_matchmaking(
                        short_member_guid,
                        characters_table_write_handle,
                        zones_table_write_handle,
                        &mut minigame_data_write_handle,
                        group.stage_group_guid,
                        &stage_config,
                        is_matchmaking,
                        matchmaking_removal_message_id,
                        game_server,
                    )
                });

            // Don't error for this player if there's an issue with another player
            match other_player_leave_result {
                Ok(mut leave_broadcasts) => broadcasts.append(&mut leave_broadcasts),
                Err(err) => info!(
                    "Unable to remove other player {member_guid} from minigame (stage group {}, stage {}) that does not have enough players: {err}",
                    group.stage_group_guid,
                    group.stage_guid
                ),
            }
        }

        drop(minigame_data_write_handle);
        minigame_data_table_write_handle.remove(group);
    }

    Ok(broadcasts)
}

fn evaluate_score_to_credits_expression(
    score_to_credits_expression: &str,
    score: i32,
) -> Result<i32, Error> {
    let context = context_map! {
        "x" => evalexpr::Value::Float(score as f64),
    }
    .unwrap_or_else(|_| panic!("Couldn't build expression evaluation context for score {score}"));

    let result = eval_with_context(score_to_credits_expression, &context).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Unable to evaluate score-to-credits expression for score {score}: {err}"),
        )
    })?;

    let Value::Float(credits) = result else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Score-to-credits expression did not return an integer for score {score}, returned: {result}"
            ),
        ));
    };

    i32::try_from(credits.round() as i64).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "Score-to-credits expression returned float that could not be converted to an integer for score {score}: {credits}, {err}"
            ),
        )
    })
}

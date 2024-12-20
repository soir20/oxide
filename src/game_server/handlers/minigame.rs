use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Error, ErrorKind, Read},
    path::Path,
    sync::Arc,
};

use byteorder::ReadBytesExt;
use evalexpr::{context_map, eval_with_context, Value};
use packet_serialize::DeserializePacket;
use serde::Deserialize;

use crate::{
    game_server::{
        handlers::character::MinigameStatus,
        packets::{
            client_update::UpdateCredits,
            command::StartFlashGame,
            minigame::{
                ActiveMinigameCreationResult, ActiveMinigameEndScore, CreateActiveMinigame,
                CreateMinigameStageGroupInstance, EndActiveMinigame, FlashPayload,
                LeaveActiveMinigame, MinigameDefinitions, MinigameHeader, MinigameOpCode,
                MinigamePortalCategory, MinigamePortalEntry, MinigameStageDefinition,
                MinigameStageGroupDefinition, MinigameStageGroupLink, MinigameStageInstance,
                RequestCancelActiveMinigame, RequestCreateActiveMinigame,
                RequestMinigameStageGroupInstance, RequestStartActiveMinigame,
                ShowStageInstanceSelect, StartActiveMinigame, UpdateActiveMinigameRewards,
            },
            tunnel::TunneledPacket,
            GamePacket, RewardBundle,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    info, teleport_to_zone,
};

use super::{
    character::{CharacterType, Player},
    lock_enforcer::{CharacterLockRequest, CharacterTableWriteHandle, ZoneTableWriteHandle},
    unique_guid::player_guid,
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

#[derive(Deserialize)]
pub struct MinigameStageConfig {
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
    pub flash_game: Option<String>,
    pub zone_template_guid: u8,
    pub score_to_credits_expression: String,
}

impl MinigameStageConfig {
    pub fn has_completed(&self, player: &Player) -> bool {
        player.minigame_stats.has_completed(self.guid)
    }

    pub fn unlocked(&self, player: &Player, previous_completed: bool) -> bool {
        self.required_item_guid
            .map(|item_guid| player.inventory.contains(&item_guid))
            .unwrap_or(true)
            && (previous_completed || !self.require_previous_completed)
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
}

#[derive(Deserialize)]
pub enum MinigameStageGroupChild {
    StageGroup(Arc<MinigameStageGroupConfig>),
    Stage(MinigameStageConfig),
}

const fn default_true() -> bool {
    true
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
                        link_name: "group".to_string(),
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
        default_stage_guid_override: Option<i32>,
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
                        link_name: "group".to_string(),
                        short_name: stage_group.short_name.clone(),
                        unlocked,
                        unknown6: 0,
                        name_id: stage_group.name_id,
                        description_id: stage_group.description_id,
                        icon_id: stage_group.stage_icon_id,
                        parent_minigame_id: 0,
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
                    let unlocked = stage.unlocked(player, previous_completed);
                    previous_completed = player.minigame_stats.has_completed(stage.guid);

                    stage_instances.push(MinigameStageInstance {
                        stage_instance_guid: stage.guid,
                        portal_entry_guid,
                        link_name: stage.link_name.clone(),
                        short_name: stage.short_name.clone(),
                        unlocked,
                        unknown6: 0,
                        name_id: stage.name_id,
                        description_id: stage.description_id,
                        icon_id: stage.stage_icon_id,
                        parent_minigame_id: 0,
                        members_only: stage.members_only,
                        unknown12: 0,
                        background_swf: "".to_string(),
                        min_players: stage.min_players,
                        max_players: stage.max_players,
                        stage_number,
                        required_item_id: 0,
                        unknown18: 0,
                        completed: previous_completed,
                        stage_group_instance_guid: 0,
                    });
                }
            }
        }

        CreateMinigameStageGroupInstance {
            header: MinigameHeader {
                stage_guid: -1,
                unknown2: -1,
                stage_group_guid: self.guid,
            },
            stage_group_guid: self.guid,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_id: self.icon_id,
            stage_select_map_name: self.stage_select_map_name.clone(),
            default_stage_instance_guid: default_stage_guid_override
                .unwrap_or(self.default_stage_instance),
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
                unknown2: -1,
                stage_group_guid: -1,
            },
            stages,
            stage_groups,
            portal_entries,
            portal_categories,
        }
    }
}

pub struct StageConfigRef<'a> {
    pub stage_config: &'a MinigameStageConfig,
    pub stage_number: u32,
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
        default_stage_guid_override: Option<i32>,
        player: &Player,
    ) -> Result<CreateMinigameStageGroupInstance, ProcessPacketError> {
        if let Some((stage_group, portal_entry_guid)) = self.stage_groups.get(&stage_group_guid) {
            Ok(stage_group.to_stage_group_instance(
                *portal_entry_guid,
                default_stage_guid_override,
                player,
            ))
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
                            if stage.guid == stage_guid {
                                Some(StageConfigRef {
                                    stage_config: stage,
                                    stage_number: index as u32 + 1,
                                    portal_entry_guid: *portal_entry_guid,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
            })
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
                handle_request_cancel_active_minigame(request, sender, game_server)
            }
            MinigameOpCode::FlashPayload => {
                let payload = FlashPayload::deserialize(cursor)?;
                handle_flash_payload(payload, sender, game_server)
            }
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                info!(
                    "Unimplemented minigame op code: {:?} {:x?}",
                    op_code, buffer
                );
                Ok(Vec::new())
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            info!("Unknown minigame packet: {}, {:x?}", raw_op_code, buffer);
            Ok(Vec::new())
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
                                        None,
                                        player,
                                    )?,
                                })?,
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: ShowStageInstanceSelect {
                                        header: MinigameHeader {
                                            stage_guid: -1,
                                            unknown2: -1,
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

fn handle_request_create_active_minigame(
    request: RequestCreateActiveMinigame,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    if let Some(StageConfigRef {
        stage_config,
        portal_entry_guid,
        ..
    }) = game_server
        .minigames()
        .stage_config(request.header.stage_group_guid, request.header.stage_guid)
    {
        let result: Result<Vec<Broadcast>, ProcessPacketError> = game_server.lock_enforcer().write_characters(|characters_table_write_handle, zones_lock_enforcer| {
            zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                // TODO: Handle multiplayer minigames and wait for a full group
                // TODO: Check to make sure the player is allowed to play this game
                let new_instance_guid = game_server.get_or_create_instance(characters_table_write_handle, zones_table_write_handle, stage_config.zone_template_guid, 1)?;
                let teleport_broadcasts = teleport_to_zone!(
                    characters_table_write_handle,
                    sender,
                    &zones_table_write_handle.get(new_instance_guid)
                        .unwrap_or_else(|| panic!("Zone instance {} should have been created or already exist but is missing", new_instance_guid))
                        .read(),
                    None,
                    None,
                    game_server.mounts(),
                    true,
                );

                if let Some(character_lock) = characters_table_write_handle.get(player_guid(sender)) {
                    if let CharacterType::Player(player) = &mut character_lock.write().stats.character_type {
                        player.minigame_status = Some(MinigameStatus {
                            stage_group_guid: request.header.stage_group_guid,
                            stage_guid: request.header.stage_guid,
                            game_created: false,
                            score_entries: vec![],
                            total_score: 0,
                        });
                        teleport_broadcasts
                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} tried to create an active minigame, but their character isn't a player", sender)))
                    }
                } else {
                    Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} tried to create an active minigame", sender)))
                }
            })
        });

        let mut broadcasts = vec![];

        if let Ok(mut teleport_broadcasts) = result {
            broadcasts.append(&mut teleport_broadcasts);
            broadcasts.push(Broadcast::Single(
                sender,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: CreateActiveMinigame {
                        header: MinigameHeader {
                            stage_guid: request.header.stage_guid,
                            unknown2: -1,
                            stage_group_guid: request.header.stage_group_guid,
                        },
                        name_id: stage_config.name_id,
                        icon_set_id: stage_config.start_screen_icon_id,
                        description_id: stage_config.description_id,
                        difficulty: stage_config.difficulty,
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
                        stage_definition_guid: request.header.stage_guid,
                        unknown22: false,
                        unknown23: false,
                        unknown24: false,
                        unknown25: 0,
                        unknown26: 0,
                        unknown27: 0,
                    },
                })?],
            ));
        } else {
            broadcasts.push(Broadcast::Single(
                sender,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: ActiveMinigameCreationResult {
                        header: MinigameHeader {
                            stage_guid: request.header.stage_guid,
                            unknown2: -1,
                            stage_group_guid: request.header.stage_group_guid,
                        },
                        was_successful: false,
                    },
                })?],
            ))
        }
        Ok(broadcasts)
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
                            let mut stage_group_instance = game_server.minigames.stage_group_instance(minigame_status.stage_group_guid, Some(minigame_status.stage_guid), player)?;
                            stage_group_instance.header.stage_guid = minigame_status.stage_guid;

                            let mut packets = vec![
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: stage_group_instance,
                                })?,
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: StartActiveMinigame {
                                        header: MinigameHeader {
                                            stage_guid: minigame_status.stage_guid,
                                            unknown2: -1,
                                            stage_group_guid: minigame_status.stage_group_guid,
                                        },
                                    },
                                })?,
                            ];

                            if let Some(StageConfigRef {stage_config, ..}) = game_server.minigames().stage_config(minigame_status.stage_group_guid, minigame_status.stage_guid) {
                                if let Some(flash_game) = &stage_config.flash_game {
                                    packets.push(
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: StartFlashGame {
                                                loader_script_name: "MiniGameFlash".to_string(),
                                                game_swf_name: flash_game.clone(),
                                                is_micro: false,
                                            },
                                        })?,
                                    );
                                }
                            } else {
                                return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} requested to start active minigame with stage config {} (stage group {}) that does not exist", sender, minigame_status.stage_guid, minigame_status.stage_group_guid)));
                            }

                            Ok(vec![
                                Broadcast::Single(sender, packets)
                            ])
                        } else {
                            info!("Player {} requested to start an active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})", sender, request.header.stage_guid, minigame_status.stage_group_guid, minigame_status.stage_guid);
                            Ok(vec![])
                        }
                    } else {
                        info!("Player {} requested to start an active minigame (stage {}), but they aren't in an active minigame", sender, request.header.stage_guid);
                        Ok(vec![])
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
    request: RequestCancelActiveMinigame,
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
                    &request,
                    false,
                    game_server,
                )
            })
        },
    )
}

fn handle_flash_payload_read_only<T: Default>(
    sender: u32,
    game_server: &GameServer,
    header: &MinigameHeader,
    func: impl FnOnce(&Player, &MinigameStatus, StageConfigRef) -> Result<T, ProcessPacketError>,
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
                                Ok(func(player, minigame_status, stage_config_ref)?)
                            } else {
                                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to process Flash payload for {}'s active minigame with stage config {} (stage group {}) that does not exist", sender, minigame_status.stage_guid, minigame_status.stage_group_guid)))
                            }
                        } else {
                            info!("Tried to process Flash payload for {}'s active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})", sender, header.stage_guid, minigame_status.stage_group_guid, minigame_status.stage_guid);
                            Ok(T::default())
                        }
                    } else {
                        info!("Tried to process Flash payload for {}'s active minigame (stage {}), but they aren't in an active minigame", sender, header.stage_guid);
                        Ok(T::default())
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

fn handle_flash_payload(
    payload: FlashPayload,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let parts: Vec<&str> = payload.payload.split('\t').collect();
    if parts.is_empty() {
        return Ok(vec![]);
    }

    match &*payload.payload {
        "FRServer_RequestStageId" => handle_flash_payload_read_only(
            sender,
            game_server,
            &payload.header,
            |_, minigame_status, stage_config_ref| {
                Ok(vec![Broadcast::Single(
                    sender,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: FlashPayload {
                            header: MinigameHeader {
                                stage_guid: minigame_status.stage_guid,
                                unknown2: -1,
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
        _ => {
            info!(
                "Received unknown Flash payload {} in stage {}, stage group {} from player {}",
                payload.payload, payload.header.stage_guid, payload.header.stage_group_guid, sender
            );
            Ok(vec![])
        }
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
                            unknown2: -1,
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
                            unknown2: -1,
                            stage_group_guid: minigame_status.stage_group_guid,
                        },
                        name_id: stage_config.name_id,
                        icon_set_id: stage_config.start_screen_icon_id,
                        description_id: stage_config.description_id,
                        difficulty: stage_config.difficulty,
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

pub fn end_active_minigame(
    sender: u32,
    characters_table_write_handle: &mut CharacterTableWriteHandle<'_>,
    zones_table_write_handle: &mut ZoneTableWriteHandle<'_>,
    request: &RequestCancelActiveMinigame,
    won: bool,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let (mut broadcasts, previous_location) = if let Some(character_lock) =
        characters_table_write_handle.get(player_guid(sender))
    {
        let mut character_write_handle = character_lock.write();
        if let CharacterType::Player(player) = &mut character_write_handle.stats.character_type {
            let previous_minigame_status = player.minigame_status.take();
            let previous_location = player.previous_location.clone();

            if let Some(minigame_status) = previous_minigame_status {
                if request.header.stage_guid == minigame_status.stage_guid {
                    if let Some(StageConfigRef { stage_config, .. }) = game_server
                        .minigames()
                        .stage_config(minigame_status.stage_group_guid, minigame_status.stage_guid)
                    {
                        let added_credits = evaluate_score_to_credits_expression(
                            &stage_config.score_to_credits_expression,
                            minigame_status.total_score,
                        )?;
                        let new_credits = player.credits.saturating_add(added_credits);
                        player.credits = new_credits;

                        let broadcasts = vec![Broadcast::Single(
                            sender,
                            vec![
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: ActiveMinigameEndScore {
                                        header: MinigameHeader {
                                            stage_guid: minigame_status.stage_guid,
                                            unknown2: -1,
                                            stage_group_guid: minigame_status.stage_group_guid,
                                        },
                                        scores: minigame_status.score_entries.clone(),
                                        unknown2: true,
                                    },
                                })?,
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: UpdateCredits { new_credits },
                                })?,
                                GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: UpdateActiveMinigameRewards {
                                        header: MinigameHeader {
                                            stage_guid: minigame_status.stage_guid,
                                            unknown2: -1,
                                            stage_group_guid: minigame_status.stage_group_guid,
                                        },
                                        reward_bundle1: RewardBundle {
                                            unknown1: false,
                                            credits: added_credits,
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
                                            unknown2: -1,
                                            stage_group_guid: minigame_status.stage_group_guid,
                                        },
                                        won,
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
                                            unknown2: -1,
                                            stage_group_guid: minigame_status.stage_group_guid,
                                        },
                                    },
                                })?,
                            ],
                        )];

                        Ok((broadcasts, previous_location))
                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to end player {}'s active minigame with stage config {} (stage group {}) that does not exist", sender, minigame_status.stage_guid, minigame_status.stage_group_guid)))
                    }
                } else {
                    info!("Tried to end player {}'s active minigame (stage {}), but they're in a different minigame (stage group {}, stage {})", sender, request.header.stage_guid, minigame_status.stage_group_guid, minigame_status.stage_guid);
                    Ok((vec![], previous_location))
                }
            } else {
                info!("Tried to end player {}'s active minigame (stage {}), but they aren't in an active minigame", sender, request.header.stage_guid);
                Ok((vec![], previous_location))
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
    }?;

    let instance_guid = game_server.get_or_create_instance(
        characters_table_write_handle,
        zones_table_write_handle,
        previous_location.template_guid,
        1,
    )?;
    let teleport_broadcasts: Result<Vec<Broadcast>, ProcessPacketError> = teleport_to_zone!(
        characters_table_write_handle,
        sender,
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
        false,
    );
    broadcasts.append(&mut teleport_broadcasts?);

    Ok(broadcasts)
}

fn evaluate_score_to_credits_expression(
    score_to_credits_expression: &str,
    score: i32,
) -> Result<u32, Error> {
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
        u32::try_from(credits.round() as i64).map_err(|err| {
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

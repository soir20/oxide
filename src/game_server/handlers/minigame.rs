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
        packets::{
            minigame::{
                CreateMinigameStageGroupInstance, MinigameDefinitions, MinigameHeader,
                MinigameOpCode, MinigamePortalCategory, MinigamePortalEntry,
                MinigameStageDefinition, MinigameStageGroupDefinition, MinigameStageGroupLink,
                MinigameStageInstance, RequestMinigameStageGroupInstance, ShowStageInstanceSelect,
            },
            tunnel::TunneledPacket,
            GamePacket,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    info,
};

use super::{
    character::{CharacterType, Player},
    lock_enforcer::CharacterLockRequest,
    unique_guid::player_guid,
};

#[derive(Clone)]
pub struct PlayerStageStats {
    pub completed: bool,
    pub high_score: i32,
}

#[derive(Clone, Default)]
pub struct PlayerMinigameStats {
    stage_guid_to_stats: BTreeMap<u32, PlayerStageStats>,
    trophy_stats: BTreeMap<i32, i32>,
}

impl PlayerMinigameStats {
    pub fn complete(&mut self, stage_guid: u32, score: i32) {
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

    pub fn has_completed(&self, stage_guid: u32) -> bool {
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
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_id: u32,
    pub min_players: u32,
    pub max_players: u32,
    pub difficulty: u32,
    pub start_sound_id: u32,
    pub required_item_guid: Option<u32>,
    pub members_only: bool,
    pub link_name: String,
    pub short_name: String,
    pub score_to_credits_expression: String,
}

impl MinigameStageConfig {
    pub fn to_stage_definition(&self, portal_entry_guid: u32) -> MinigameStageDefinition {
        MinigameStageDefinition {
            guid: self.guid,
            portal_entry_guid,
            start_screen_name_id: self.name_id,
            start_screen_description_id: self.description_id,
            start_screen_icon_set_id: self.icon_id,
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
pub struct MinigameStageGroupConfig {
    pub guid: i32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_id: u32,
    pub stage_select_map_name: String,
    #[serde(default)]
    pub default_stage_instance: u32,
    pub child_stage_groups: Vec<Arc<MinigameStageGroupConfig>>,
    pub stages: Vec<MinigameStageConfig>,
}

impl MinigameStageGroupConfig {
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

        let mut stage_number = 1;
        for stage in &self.stages {
            stages.push(stage.to_stage_definition(portal_entry_guid));
            group_links.push(MinigameStageGroupLink {
                link_id: 0,
                stage_group_definition_guid: self.guid,
                parent_game_id: 0,
                link_stage_definition_guid: stage.guid,
                icon_id: 0,
                link_name: stage.link_name.clone(),
                short_name: stage.short_name.clone(),
                stage_number,
                link_stage_group_definition_guid: 0,
            });
            stage_number += 1;
        }

        for stage_group in &self.child_stage_groups {
            let (mut stage_group_definitions, mut stage_definitions) =
                stage_group.to_stage_group_definition(portal_entry_guid);
            stage_groups.append(&mut stage_group_definitions);
            stages.append(&mut stage_definitions);

            group_links.push(MinigameStageGroupLink {
                link_id: 0,
                stage_group_definition_guid: self.guid,
                parent_game_id: 0,
                link_stage_definition_guid: 0,
                icon_id: 0,
                link_name: "group".to_string(),
                short_name: "".to_string(),
                stage_number,
                link_stage_group_definition_guid: stage_group.guid,
            });
        }

        stage_groups.push(MinigameStageGroupDefinition {
            guid: self.guid,
            portal_entry_guid,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_set_id: self.icon_id,
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
        default_stage_guid_override: Option<u32>,
        player: &Player,
    ) -> CreateMinigameStageGroupInstance {
        let mut stage_instances = Vec::new();
        let mut stage_number = 1;
        let mut previous_completed = true;

        for stage in &self.stages {
            let completed = player.minigame_stats.has_completed(stage.guid);
            let unlocked = stage
                .required_item_guid
                .map(|item_guid| player.inventory.contains(&item_guid))
                .unwrap_or(true)
                && previous_completed;

            stage_instances.push(MinigameStageInstance {
                stage_instance_guid: stage.guid,
                portal_entry_guid,
                link_name: stage.link_name.clone(),
                short_name: stage.short_name.clone(),
                unlocked,
                unknown6: 0,
                name_id: stage.name_id,
                description_id: stage.description_id,
                icon_set_id: stage.icon_id,
                parent_minigame_id: 0,
                members_only: stage.members_only,
                unknown12: 0,
                background_swf: "".to_string(),
                min_players: stage.min_players,
                max_players: stage.max_players,
                stage_number,
                required_item_id: 0,
                unknown18: 0,
                completed,
                link_group_id: 0,
            });
            stage_number += 1;
            previous_completed = completed;
        }

        CreateMinigameStageGroupInstance {
            header: MinigameHeader {
                active_minigame_guid: -1,
                unknown2: -1,
                stage_group_guid: self.guid,
            },
            stage_group_guid: self.guid,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_set_id: self.icon_id,
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
    pub is_micro: bool,
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
                is_micro: self.is_micro,
                is_active: self.is_active,
                param1: self.param1,
                icon_set_id: self.icon_id,
                background_icon_set_id: self.background_icon_id,
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
                icon_set_id: value.icon_id,
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
                active_minigame_guid: -1,
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
        default_stage_guid_override: Option<u32>,
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
}

impl From<Vec<MinigamePortalCategoryConfig>> for AllMinigameConfigs {
    fn from(value: Vec<MinigamePortalCategoryConfig>) -> Self {
        let mut stage_groups = BTreeMap::new();
        for category in &value {
            for entry in &category.portal_entries {
                stage_groups.insert(
                    entry.stage_group.guid,
                    (entry.stage_group.clone(), entry.guid),
                );

                for stage_group in &entry.stage_group.child_stage_groups {
                    stage_groups.insert(stage_group.guid, (stage_group.clone(), entry.guid));
                }
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

                game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
                    read_guids: vec![player_guid(sender)],
                    write_guids: Vec::new(),
                    character_consumer: |_, characters_read, _, _| {
                        if let Some(character_read_handle) = characters_read.get(&player_guid(sender)) {
                            if let CharacterType::Player(player) = &character_read_handle.stats.character_type {
                                Ok(vec![Broadcast::Single(
                                    sender,
                                    vec![
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: game_server.minigames().stage_group_instance(request.header.stage_group_guid, None, player)?,
                                        })?,
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: ShowStageInstanceSelect {
                                                header: MinigameHeader {
                                                    active_minigame_guid: -1,
                                                    unknown2: -1,
                                                    stage_group_guid: request.header.stage_group_guid
                                                }
                                            },
                                        })?,
                                    ],
                                )])
                            } else {
                                Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Non-player character {} requested a stage group instance {}", sender, request.header.stage_group_guid)))
                            }
                        } else {
                            Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown character {} requested a stage group instance {}", sender, request.header.stage_group_guid)))
                        }
                    },
                })
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

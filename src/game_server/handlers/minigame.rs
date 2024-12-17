use std::io::{Error, ErrorKind};

use evalexpr::{context_map, eval_with_context, Value};
use serde::Deserialize;

use crate::game_server::packets::minigame::{
    MinigameDefinitions, MinigameHeader, MinigamePortalCategory, MinigamePortalEntry,
    MinigameStageDefinition, MinigameStageGroupDefinition, MinigameStageGroupLink,
};

#[derive(Deserialize)]
pub struct MinigameStageConfig {
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub min_players: u32,
    pub max_players: u32,
    pub difficulty: u32,
    pub start_sound_id: u32,
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
            start_screen_icon_set_id: self.icon_set_id,
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
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub stage_select_map_name: String,
    pub default_stage_instance: u32,
    pub stages: Vec<MinigameStageConfig>,
}

impl MinigameStageGroupConfig {
    pub fn to_stage_group_definition(
        &self,
        portal_entry_guid: u32,
    ) -> (MinigameStageGroupDefinition, Vec<MinigameStageDefinition>) {
        let mut stages = Vec::new();
        let mut group_links = Vec::new();

        for stage in &self.stages {
            stages.push(stage.to_stage_definition(portal_entry_guid));
            group_links.push(MinigameStageGroupLink {
                link_id: stage.guid,
                stage_group_definition_guid: self.guid,
                parent_game_id: 0,
                link_stage_definition_guid: stage.guid,
                unknown5: 0,
                link_name: stage.link_name.clone(),
                short_name: stage.short_name.clone(),
                unknown8: 0,
                unknown9: 0,
            })
        }

        (
            MinigameStageGroupDefinition {
                guid: self.guid,
                portal_entry_guid,
                name_id: self.name_id,
                description_id: self.description_id,
                icon_set_id: self.icon_set_id,
                stage_select_map_name: self.stage_select_map_name.clone(),
                stage_progression: "".to_string(),
                show_start_screen_on_play_next: false,
                unknown9: 0,
                unknown10: 0,
                unknown11: 0,
                unknown12: 0,
                unknown13: 0,
                group_links,
            },
            stages,
        )
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
    pub icon_set_id: u32,
    pub background_icon_set_id: u32,
    pub is_popular: bool,
    pub is_game_of_day: bool,
    pub portal_category_guid: u32,
    pub sort_order: u32,
    pub tutorial_swf: String,
    pub stage_groups: Vec<MinigameStageGroupConfig>,
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

        for stage_group in &self.stage_groups {
            let (group_definition, mut stage_definitions) =
                stage_group.to_stage_group_definition(self.guid);
            stage_groups.push(group_definition);
            stages.append(&mut stage_definitions);
        }

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
                icon_set_id: self.icon_set_id,
                background_icon_set_id: self.background_icon_set_id,
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
    pub icon_set_id: u32,
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
                icon_set_id: value.icon_set_id,
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

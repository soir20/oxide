use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    path::Path,
};

use crate::{
    game_server::{
        handlers::{
            ability::AbilityConfig,
            store::{compute_costs, ItemCostMap},
        },
        packets::{
            item::{ItemDefinition, ItemType, SpecialItemAbility},
            player_update::CustomizationSlot,
        },
    },
    ConfigError,
};

use serde::Deserialize;
use walkdir::WalkDir;

pub const SABER_ITEM_TYPE: u32 = 25;

const DEFAULT_COST_EXPRESSION: &str = "max(0.9*x - 1, 0.0)";

fn default_members_cost_expression() -> String {
    DEFAULT_COST_EXPRESSION.to_string()
}

const fn default_item_class() -> i32 {
    -1
}

const fn default_stack_size() -> i32 {
    1
}

#[derive(PartialEq, Clone, Default, Debug, Deserialize)]
pub struct ItemActionBarConfig {
    pub priority_override: Option<u32>,
    pub ability_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemConfig {
    pub guid: u32,
    #[serde(default)]
    pub name_id: u32,
    #[serde(default)]
    pub description_id: u32,
    #[serde(default)]
    pub icon_set_id: u32,
    #[serde(default)]
    pub tint: u32,
    #[serde(default)]
    pub cost: u32,
    #[serde(default = "default_members_cost_expression")]
    pub members_cost_expression: String,
    #[serde(default = "default_item_class")]
    pub item_class: i32,
    #[serde(default)]
    pub required_battle_class: u32,
    pub slot: ItemType,
    #[serde(default)]
    pub disable_trade: bool,
    #[serde(default)]
    pub disable_sale: bool,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub texture_alias: String,
    #[serde(default)]
    pub required_gender: u32,
    pub item_type: u32,
    #[serde(default)]
    pub category: u32,
    #[serde(default)]
    pub members: bool,
    #[serde(default)]
    pub non_minigame: bool,
    pub weapon_trail_effect: Option<u32>,
    pub composite_effect: Option<u32>,
    #[serde(default)]
    pub power_rating: u32,
    #[serde(default)]
    pub min_battle_class_level: u32,
    #[serde(default)]
    pub rarity: u32,
    #[serde(default)]
    pub single_use: bool,
    #[serde(default = "default_stack_size")]
    pub max_stack_size: i32,
    #[serde(default)]
    pub tint_alias: String,
    #[serde(default)]
    pub disable_preview: bool,
    #[serde(default)]
    pub race_set_id: u32,
    #[serde(default)]
    pub customization_slot: CustomizationSlot,
    #[serde(default)]
    pub customization_id: u32,
    #[serde(default)]
    pub action_bar: ItemActionBarConfig,
}

impl ItemConfig {
    fn resolve_special_abilities(
        &self,
        abilities: &HashMap<String, AbilityConfig>,
    ) -> Vec<SpecialItemAbility> {
        self.action_bar
            .ability_keys
            .iter()
            .enumerate()
            .filter_map(|(index, key)| {
                // Only indexes 1–3 are special
                if index == 0 {
                    return None;
                }

                let ability = abilities.get(key).unwrap_or_else(|| {
                    panic!("Item {} references unknown ability key {}", self.guid, key)
                });

                Some(SpecialItemAbility {
                    ability_id: 0,
                    ability_slot: index as u32,
                    unknown3: 0,
                    ability_icon: ability.icon_set_id,
                    unknown5: 0,
                    unknown6: 0,
                    ability_name: ability.name_id,
                })
            })
            .collect()
    }

    pub fn to_definition(&self, abilities: &HashMap<String, AbilityConfig>) -> ItemDefinition {
        ItemDefinition {
            guid: self.guid,
            name_id: self.name_id,
            description_id: self.description_id,
            icon_set_id: self.icon_set_id,
            tint: self.tint,
            unknown6: 0,
            unknown7: 0,
            cost: self.cost,
            item_class: self.item_class,
            required_battle_class: self.required_battle_class,
            slot: self.slot,
            disable_trade: self.disable_trade,
            disable_sale: self.disable_sale,
            model_name: self.model_name.clone(),
            texture_alias: self.texture_alias.clone(),
            required_gender: self.required_gender,
            item_type: self.item_type,
            category: self.category,
            members: self.members,
            non_minigame: self.non_minigame,
            weapon_trail_effect: self.weapon_trail_effect.unwrap_or_default(),
            composite_effect: self.composite_effect.unwrap_or_default(),
            power_rating: self.power_rating,
            min_battle_class_level: self.min_battle_class_level,
            rarity: self.rarity,
            activatable_ability_id: 0,
            passive_ability_id: 0,
            single_use: self.single_use,
            max_stack_size: self.max_stack_size,
            is_tintable: !self.tint_alias.trim().is_empty(),
            tint_alias: self.tint_alias.clone(),
            disable_preview: self.disable_preview,
            unknown33: false,
            race_set_id: self.race_set_id,
            unknown35: false,
            unknown36: 0,
            unknown37: 0,
            customization_slot: self.customization_slot,
            customization_id: self.customization_id,
            unknown40: 0,
            stats: vec![],
            special_abilities: self.resolve_special_abilities(abilities),
        }
    }
}

pub type ItemConfigMap = BTreeMap<u32, ItemConfig>;

pub fn load_items(
    config_dir: &Path,
    abilities: &HashMap<String, AbilityConfig>,
) -> Result<(ItemConfigMap, ItemCostMap), ConfigError> {
    let items_dir = config_dir.join("items");

    let yaml_files = WalkDir::new(&items_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            entry.file_type().is_file() && path.extension().is_some_and(|ext| ext == "yaml")
        })
        .map(|entry| entry.into_path());

    let mut items = BTreeMap::new();
    let mut item_paths = HashMap::new();
    let mut costs = BTreeMap::new();

    for file_path in yaml_files {
        let file = File::open(&file_path)?;
        let configs: Vec<ItemConfig> = serde_yaml::from_reader(file)?;
        costs.extend(compute_costs(&configs)?);

        for config in configs {
            let ability_count = config.action_bar.ability_keys.len();
            if ability_count > 4 {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Item {} has {} abilities in file {:?} (max 4)",
                    config.guid, ability_count, file_path
                )));
            }

            for key in &config.action_bar.ability_keys {
                if !abilities.contains_key(key) {
                    return Err(ConfigError::ConstraintViolated(format!(
                        "Item {} references unknown ability key {} in file {:?}",
                        config.guid, key, file_path
                    )));
                }
            }

            let guid = config.guid;

            if let Some(previous) = items.insert(guid, config) {
                let first_path = item_paths.get(&previous.guid).unwrap();
                return Err(ConfigError::ConstraintViolated(format!(
                    "Item {} has conflicting definitions:\n  - first seen in: {:?}\n  - duplicate found in: {:?}",
                    previous.guid,
                    first_path,
                    file_path
                )));
            }

            item_paths.insert(guid, file_path.clone());
        }
    }

    Ok((items, costs))
}

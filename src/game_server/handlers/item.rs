use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    path::Path,
};

use crate::{
    game_server::{
        handlers::ability::PlayerAbility,
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

pub struct PlayerItemActionBar {
    pub priority: u32,
    pub ability_ids: Vec<u32>,
}

pub struct PlayerItem {
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub tint: u32,
    pub cost: u32,
    pub item_class: i32,
    pub required_battle_class: u32,
    pub slot: ItemType,
    pub disable_trade: bool,
    pub disable_sale: bool,
    pub model_name: String,
    pub texture_alias: String,
    pub required_gender: u32,
    pub item_type: u32,
    pub category: u32,
    pub members: bool,
    pub non_minigame: bool,
    pub weapon_trail_effect: Option<u32>,
    pub composite_effect: Option<u32>,
    pub power_rating: u32,
    pub min_battle_class_level: u32,
    pub rarity: u32,
    pub single_use: bool,
    pub max_stack_size: i32,
    pub tint_alias: String,
    pub disable_preview: bool,
    pub race_set_id: u32,
    pub customization_slot: CustomizationSlot,
    pub customization_id: u32,
    pub action_bar: PlayerItemActionBar,
}

impl PlayerItem {
    fn to_special_abilities(
        &self,
        abilities: &BTreeMap<u32, PlayerAbility>,
    ) -> Vec<SpecialItemAbility> {
        self.action_bar
            .ability_ids
            .iter()
            .enumerate()
            .filter_map(|(index, ability_id)| {
                // Only indexes 1-3 are considered special
                if index == 0 {
                    return None;
                }

                let ability = abilities.get(ability_id).unwrap_or_else(|| {
                    panic!(
                        "Item {} references unknown ability {}",
                        self.guid, ability_id
                    )
                });

                Some(SpecialItemAbility {
                    ability_id: *ability_id,
                    ability_slot: index as u32,
                    unknown3: 0,
                    ability_icon: ability.slot_info.icon_set_id,
                    unknown5: 0,
                    unknown6: 0,
                    ability_name: ability.slot_info.name_id,
                })
            })
            .collect()
    }

    pub fn from_config(config: ItemConfig, ability_keys_to_id: &HashMap<String, u32>) -> Self {
        let mut resolved_ability_ids = Vec::new();
        for key in &config.action_bar.ability_keys {
            let ability_id = ability_keys_to_id.get(key).unwrap_or_else(|| {
                panic!(
                    "Item {} references unknown ability key {}",
                    config.guid, key
                )
            });
            resolved_ability_ids.push(*ability_id);
        }
        Self {
            guid: config.guid,
            name_id: config.name_id,
            description_id: config.description_id,
            icon_set_id: config.icon_set_id,
            tint: config.tint,
            cost: config.cost,
            item_class: config.item_class,
            required_battle_class: config.required_battle_class,
            slot: config.slot,
            disable_trade: config.disable_trade,
            disable_sale: config.disable_sale,
            model_name: config.model_name,
            texture_alias: config.texture_alias,
            required_gender: config.required_gender,
            item_type: config.item_type,
            category: config.category,
            members: config.members,
            non_minigame: config.non_minigame,
            weapon_trail_effect: config.weapon_trail_effect,
            composite_effect: config.composite_effect,
            power_rating: config.power_rating,
            min_battle_class_level: config.min_battle_class_level,
            rarity: config.rarity,
            single_use: config.single_use,
            max_stack_size: config.max_stack_size,
            tint_alias: config.tint_alias,
            disable_preview: config.disable_preview,
            race_set_id: config.race_set_id,
            customization_slot: config.customization_slot,
            customization_id: config.customization_id,
            action_bar: PlayerItemActionBar {
                priority: config.action_bar.priority_override.unwrap_or_else(|| {
                    match config.slot {
                        ItemType::Equipment(slot) => slot.action_bar_priority(),
                        ItemType::Customization(_) => 0,
                    }
                }),
                ability_ids: resolved_ability_ids,
            },
        }
    }
}

impl ItemDefinition {
    pub fn from_player_item(
        player_item: &PlayerItem,
        abilities: &BTreeMap<u32, PlayerAbility>,
    ) -> Self {
        ItemDefinition {
            guid: player_item.guid,
            name_id: player_item.name_id,
            description_id: player_item.description_id,
            icon_set_id: player_item.icon_set_id,
            tint: player_item.tint,
            unknown6: 0,
            unknown7: 0,
            cost: player_item.cost,
            item_class: player_item.item_class,
            required_battle_class: player_item.required_battle_class,
            slot: player_item.slot,
            disable_trade: player_item.disable_trade,
            disable_sale: player_item.disable_sale,
            model_name: player_item.model_name.clone(),
            texture_alias: player_item.texture_alias.clone(),
            required_gender: player_item.required_gender,
            item_type: player_item.item_type,
            category: player_item.category,
            members: player_item.members,
            non_minigame: player_item.non_minigame,
            weapon_trail_effect: player_item.weapon_trail_effect.unwrap_or_default(),
            composite_effect: player_item.composite_effect.unwrap_or_default(),
            power_rating: player_item.power_rating,
            min_battle_class_level: player_item.min_battle_class_level,
            rarity: player_item.rarity,
            activatable_ability_id: 0,
            passive_ability_id: 0,
            single_use: player_item.single_use,
            max_stack_size: player_item.max_stack_size,
            is_tintable: !player_item.tint_alias.trim().is_empty(),
            tint_alias: player_item.tint_alias.clone(),
            disable_preview: player_item.disable_preview,
            unknown33: false,
            race_set_id: player_item.race_set_id,
            unknown35: false,
            unknown36: 0,
            unknown37: 0,
            customization_slot: player_item.customization_slot,
            customization_id: player_item.customization_id,
            unknown40: 0,
            stats: vec![],
            special_abilities: player_item.to_special_abilities(abilities),
        }
    }
}

pub fn load_item_definitions(
    config_dir: &Path,
    ability_keys_to_id: &HashMap<String, u32>,
) -> Result<BTreeMap<u32, PlayerItem>, ConfigError> {
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

    for file_path in yaml_files {
        let file = File::open(&file_path)?;
        let configs: Vec<ItemConfig> = serde_yaml::from_reader(file)?;

        for config in configs {
            let ability_count = config.action_bar.ability_keys.len();
            if ability_count > 4 {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Item {} has {} abilities in file {:?} (max 4)",
                    config.guid, ability_count, file_path
                )));
            }

            let item = PlayerItem::from_config(config, ability_keys_to_id);
            let guid = item.guid;

            if let Some(previous) = items.insert(guid, item) {
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

    Ok(items)
}

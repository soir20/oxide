use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
};

use crate::{
    game_server::packets::{
        item::{ItemAbility, ItemDefinition, ItemType},
        player_update::CustomizationSlot,
    },
    ConfigError,
};

use serde::Deserialize;

pub const SABER_ITEM_TYPE: u32 = 25;

#[derive(Debug, Deserialize)]
pub struct ItemAbilityConfig {
    pub ability_icon: u32,
    pub ability_name: u32,
}

#[derive(Debug, Deserialize)]
pub struct ItemConfig {
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
    pub weapon_trail_effect: u32,
    pub composite_effect: u32,
    pub power_rating: u32,
    pub min_battle_class_level: u32,
    pub rarity: u32,
    pub activatable_ability_id: u32,
    pub passive_ability_id: u32,
    pub single_use: bool,
    pub max_stack_size: i32,
    pub is_tintable: bool,
    pub tint_alias: String,
    pub disable_preview: bool,
    pub race_set_id: u32,
    pub customization_slot: CustomizationSlot,
    pub customization_id: u32,
    #[serde(default)]
    pub abilities: Vec<ItemAbilityConfig>,
}

impl From<ItemAbilityConfig> for ItemAbility {
    fn from(cfg: ItemAbilityConfig) -> Self {
        ItemAbility {
            ability_slot: 0,
            ability_id: 0,
            unknown3: 0,
            ability_icon: cfg.ability_icon,
            unknown5: 0,
            unknown6: 0,
            ability_name: cfg.ability_name,
        }
    }
}

impl From<ItemConfig> for ItemDefinition {
    fn from(cfg: ItemConfig) -> Self {
        ItemDefinition {
            guid: cfg.guid,
            name_id: cfg.name_id,
            description_id: cfg.description_id,
            icon_set_id: cfg.icon_set_id,
            tint: cfg.tint,
            unknown6: 0,
            unknown7: 0,
            cost: cfg.cost,
            item_class: cfg.item_class,
            required_battle_class: cfg.required_battle_class,
            slot: cfg.slot,
            disable_trade: cfg.disable_trade,
            disable_sale: cfg.disable_sale,
            model_name: cfg.model_name,
            texture_alias: cfg.texture_alias,
            required_gender: cfg.required_gender,
            item_type: cfg.item_type,
            category: cfg.category,
            members: cfg.members,
            non_minigame: cfg.non_minigame,
            weapon_trail_effect: cfg.weapon_trail_effect,
            composite_effect: cfg.composite_effect,
            power_rating: cfg.power_rating,
            min_battle_class_level: cfg.min_battle_class_level,
            rarity: cfg.rarity,
            activatable_ability_id: cfg.activatable_ability_id,
            passive_ability_id: cfg.passive_ability_id,
            single_use: cfg.single_use,
            max_stack_size: cfg.max_stack_size,
            is_tintable: cfg.is_tintable,
            tint_alias: cfg.tint_alias,
            disable_preview: cfg.disable_preview,
            unknown33: false,
            race_set_id: cfg.race_set_id,
            unknown35: false,
            unknown36: 0,
            unknown37: 0,
            customization_slot: cfg.customization_slot,
            customization_id: cfg.customization_id,
            unknown40: 0,
            stats: vec![],
            abilities: cfg.abilities.into_iter().map(ItemAbility::from).collect(),
        }
    }
}

pub fn load_item_definitions(
    config_dir: &Path,
) -> Result<BTreeMap<u32, ItemDefinition>, ConfigError> {
    let items_dir = config_dir.join("items");

    fn find_yaml_files(root: &Path) -> Result<Vec<PathBuf>, ConfigError> {
        let mut files = Vec::new();

        for entry in fs::read_dir(root)? {
            let entry_path = entry?.path();

            if entry_path.is_dir() {
                files.extend(find_yaml_files(&entry_path)?);
            } else if entry_path.extension().is_some_and(|ext| ext == "yaml") {
                files.push(entry_path);
            }
        }

        Ok(files)
    }

    let yaml_files = find_yaml_files(&items_dir)?;
    let mut items = BTreeMap::new();

    for file_path in yaml_files {
        let file = File::open(&file_path)?;

        let configs: Vec<ItemConfig> = serde_yaml::from_reader(file)?;

        for cfg in configs {
            let def: ItemDefinition = cfg.into();

            if let Some(previous) = items.insert(def.guid, def) {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Two item definitions have ID {} (file: {:?})",
                    previous.guid, file_path
                )));
            }
        }
    }

    Ok(items)
}

use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    path::Path,
};

use serde::Deserialize;

use crate::{
    game_server::{
        handlers::{item::ItemConfig, store::ItemCostMap},
        packets::reference_data::{
            CategoryDefinitions, ItemClassDefinition, ItemClassDefinitions, ItemGroupDefinition,
            ItemGroupItem,
        },
    },
    ConfigError,
};

pub fn load_item_classes(config_dir: &Path) -> Result<ItemClassDefinitions, ConfigError> {
    let mut file = File::open(config_dir.join("item_classes.yaml"))?;
    let definitions: Vec<ItemClassDefinition> = serde_yaml::from_reader(&mut file)?;
    Ok(ItemClassDefinitions {
        definitions: definitions
            .into_iter()
            .map(|definition| (definition.guid, definition))
            .collect(),
    })
}

pub fn load_categories(config_dir: &Path) -> Result<CategoryDefinitions, ConfigError> {
    let mut file = File::open(config_dir.join("item_categories.yaml"))?;
    Ok(serde_yaml::from_reader(&mut file)?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemGroupConfig {
    pub guid: i32,
    #[serde(default)]
    pub name_id: u32,
    #[serde(default)]
    pub description_id: u32,
    #[serde(default)]
    pub sort_order: u32,
    #[serde(default)]
    pub icon_set_id: u32,
    #[serde(default)]
    pub category: u32,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub preview_model_id: u32,
    #[serde(default)]
    pub preview_animation_id: i32,
    #[serde(default)]
    pub is_new: bool,
    #[serde(default)]
    pub members_only: bool,
    #[serde(default)]
    pub for_sale: bool,
    #[serde(default)]
    pub items: Vec<ItemGroupItem>,
}

impl From<ItemGroupConfig> for ItemGroupDefinition {
    fn from(value: ItemGroupConfig) -> Self {
        ItemGroupDefinition {
            guid: value.guid,
            unknown2: 0,
            name_id: value.name_id,
            description_id: value.description_id,
            sort_order: value.sort_order,
            icon_set_id: value.icon_set_id,
            category: value.category,
            page: value.page,
            preview_model_id: value.preview_model_id,
            preview_animation_id: value.preview_animation_id,
            is_new: value.is_new,
            unknown12: 0,
            unknown13: 0,
            unknown14: 0,
            unknown16: "".to_string(),
            members_only: value.members_only,
            items: value.items,
        }
    }
}

pub fn load_item_groups(
    config_dir: &Path,
    items: &BTreeMap<u32, ItemConfig>,
    costs: &mut ItemCostMap,
) -> Result<Vec<ItemGroupDefinition>, ConfigError> {
    let mut file = File::open(config_dir.join("item_groups.yaml"))?;
    let groups: Vec<ItemGroupConfig> = serde_yaml::from_reader(&mut file)?;

    for group in groups.iter() {
        for item in group.items.iter() {
            if !items.contains_key(&item.guid) {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Item group {} contains unknown item {}",
                    group.guid, item.guid
                )));
            }
        }
    }

    let items_for_sale: HashSet<u32> = groups
        .iter()
        .filter(|group| group.for_sale)
        .flat_map(|group| group.items.iter().map(|item| item.guid))
        .collect();
    costs.retain(|item_guid, _| items_for_sale.contains(item_guid));

    Ok(groups.into_iter().map(|group| group.into()).collect())
}

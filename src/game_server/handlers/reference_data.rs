use std::{fs::File, path::Path};

use crate::{
    game_server::packets::reference_data::{
        CategoryDefinitions, ItemClassDefinition, ItemClassDefinitions, ItemGroupDefinition,
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

pub fn load_item_groups(config_dir: &Path) -> Result<Vec<ItemGroupDefinition>, ConfigError> {
    let mut file = File::open(config_dir.join("item_groups.yaml"))?;
    Ok(serde_yaml::from_reader(&mut file)?)
}

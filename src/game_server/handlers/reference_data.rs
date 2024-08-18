use std::{fs::File, io::Error, path::Path};

use crate::game_server::packets::reference_data::{
    CategoryDefinitions, ItemClassDefinition, ItemClassDefinitions,
};

pub fn load_item_classes(config_dir: &Path) -> Result<ItemClassDefinitions, Error> {
    let mut file = File::open(config_dir.join("item_classes.json"))?;
    let definitions: Vec<ItemClassDefinition> = serde_json::from_reader(&mut file)?;
    Ok(ItemClassDefinitions {
        definitions: definitions
            .into_iter()
            .map(|definition| (definition.guid, definition))
            .collect(),
    })
}

pub fn load_categories(config_dir: &Path) -> Result<CategoryDefinitions, Error> {
    let mut file = File::open(config_dir.join("item_categories.json"))?;
    Ok(serde_json::from_reader(&mut file)?)
}

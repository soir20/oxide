use std::{collections::BTreeMap, fs::File, path::Path};

use crate::{game_server::packets::item::ItemDefinition, ConfigError};

pub const SABER_ITEM_TYPE: u32 = 25;

pub fn load_item_definitions(
    config_dir: &Path,
) -> Result<BTreeMap<u32, ItemDefinition>, ConfigError> {
    let mut file = File::open(config_dir.join("items.json"))?;
    let item_defs: Vec<ItemDefinition> = serde_yaml::from_reader(&mut file)?;

    let mut item_def_map = BTreeMap::new();
    for item_def in item_defs {
        if let Some(previous_item_def) = item_def_map.insert(item_def.guid, item_def) {
            panic!("Two item definitions have ID {}", previous_item_def.guid);
        }
    }
    Ok(item_def_map)
}

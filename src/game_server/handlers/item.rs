use std::{
    collections::BTreeMap,
    fs,
    fs::File,
    path::{Path, PathBuf},
};

use crate::{game_server::packets::item::ItemDefinition, ConfigError};

pub const SABER_ITEM_TYPE: u32 = 25;

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
        let item_defs: Vec<ItemDefinition> = serde_yaml::from_reader(file)?;

        for item_def in item_defs {
            if let Some(previous) = items.insert(item_def.guid, item_def) {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Two item definitions have ID {} (file: {:?})",
                    previous.guid, file_path
                )));
            }
        }
    }

    Ok(items)
}

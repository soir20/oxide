use std::{fs::File, io::Error, path::Path};

use crate::game_server::packets::reference_data::CategoryDefinitions;

pub fn load_categories(config_dir: &Path) -> Result<CategoryDefinitions, Error> {
    let mut file = File::open(config_dir.join("item_categories.json"))?;
    Ok(serde_json::from_reader(&mut file)?)
}

use std::{collections::HashSet, fs::File, path::Path};

use crate::ConfigError;

pub fn load_enemy_types(config_dir: &Path) -> Result<HashSet<String>, ConfigError> {
    let mut file = File::open(config_dir.join("enemy_types.yaml"))?;
    let enemy_types: HashSet<String> = serde_yaml::from_reader(&mut file)?;
    Ok(enemy_types)
}

pub struct EnemyPriorization {}

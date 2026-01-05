use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
};

use crate::ConfigError;

pub fn load_enemy_types(config_dir: &Path) -> Result<HashSet<String>, ConfigError> {
    let mut file = File::open(config_dir.join("enemy_types.yaml"))?;
    let enemy_types: HashSet<String> = serde_yaml::from_reader(&mut file)?;
    Ok(enemy_types)
}

pub struct EnemyPrioritization {
    priority_points_by_type: HashMap<String, i8>,
}

impl EnemyPrioritization {
    pub fn priority<'a>(&self, enemy_types: impl Iterator<Item = &'a String>) -> i8 {
        enemy_types.fold(0, |acc, enemy_type| {
            acc.saturating_add(
                self.priority_points_by_type
                    .get(enemy_type)
                    .copied()
                    .unwrap_or_default(),
            )
        })
    }
}

impl From<HashMap<String, i8>> for EnemyPrioritization {
    fn from(priority_points_by_type: HashMap<String, i8>) -> Self {
        EnemyPrioritization {
            priority_points_by_type,
        }
    }
}

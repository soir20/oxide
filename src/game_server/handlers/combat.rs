use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
};

use mut_binary_heap::BinaryHeap;

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

pub struct ThreatTable {
    heap: BinaryHeap<u64, (i8, u32)>,
    prioritization: EnemyPrioritization,
}

impl ThreatTable {
    pub fn deal_damage<'a>(
        &mut self,
        attacker_guid: u64,
        attacker_types: impl Iterator<Item = &'a String>,
        damage_dealt: u32,
    ) {
        if let Some(mut value) = self.heap.get_mut(&attacker_guid) {
            *value = (value.0, value.1.saturating_add(damage_dealt));
        }

        if !self.heap.contains_key(&attacker_guid) {
            let priority = self.prioritization.priority(attacker_types);
            self.heap.push(attacker_guid, (priority, damage_dealt));
        }
    }
}

impl From<HashMap<String, i8>> for ThreatTable {
    fn from(priority_points_by_type: HashMap<String, i8>) -> Self {
        ThreatTable {
            heap: BinaryHeap::new(),
            prioritization: priority_points_by_type.into(),
        }
    }
}

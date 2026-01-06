use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
    time::Instant,
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

#[derive(Eq, PartialEq)]
pub struct ThreatTableValue {
    pub priority: i8,
    pub damage_dealt: u32,
    pub time_added: Instant,
}

impl Ord for ThreatTableValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority
            .cmp(&other.priority)
            .then(self.damage_dealt.cmp(&other.damage_dealt))
            .then(other.time_added.cmp(&self.time_added))
    }
}

impl PartialOrd for ThreatTableValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ThreatTable {
    heap: BinaryHeap<u64, ThreatTableValue>,
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
            *value = ThreatTableValue {
                priority: value.priority,
                damage_dealt: value.damage_dealt.saturating_add(damage_dealt),
                time_added: value.time_added,
            };
        }

        if !self.heap.contains_key(&attacker_guid) {
            let priority = self.prioritization.priority(attacker_types);
            self.heap.push(
                attacker_guid,
                ThreatTableValue {
                    priority,
                    damage_dealt,
                    time_added: Instant::now(),
                },
            );
        }
    }

    pub fn remove(&mut self, guid: u64) {
        self.heap.remove(&guid);
    }

    pub fn retain<F: FnMut(&u64, &ThreatTableValue) -> bool>(&mut self, mut pred: F) {
        let keys_to_remove: Vec<u64> = self
            .heap
            .iter()
            .filter_map(|(key, value)| match pred(key, value) {
                true => None,
                false => Some(*key),
            })
            .collect();

        for key in keys_to_remove.into_iter() {
            self.remove(key);
        }
    }

    pub fn target(&self) -> Option<u64> {
        self.heap.peek_with_key().map(|(guid, _)| *guid)
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

#[cfg(test)]
mod tests {
    use std::iter;

    use super::*;

    #[test]
    fn test_priority_by_enemy_type() {
        let mut table: ThreatTable = HashMap::from_iter(vec![
            ("one".to_string(), 1),
            ("two".to_string(), 2),
            ("three".to_string(), 3),
            ("four".to_string(), 4),
        ])
        .into();
        table.deal_damage(2, ["two".to_string()].iter(), 30);
        table.deal_damage(1, ["one".to_string()].iter(), 20);
        table.deal_damage(4, ["four".to_string()].iter(), 10);
        table.deal_damage(3, ["three".to_string()].iter(), 40);

        assert_eq!(Some(4), table.target());
        table.remove(4);
        assert_eq!(Some(3), table.target());
        table.remove(3);
        assert_eq!(Some(2), table.target());
        table.remove(2);
        assert_eq!(Some(1), table.target());
        table.remove(1);
        assert_eq!(None, table.target());
    }

    #[test]
    fn test_priority_by_damage_dealt() {
        let mut table: ThreatTable = HashMap::new().into();
        table.deal_damage(2, iter::empty(), 30);
        table.deal_damage(1, iter::empty(), 20);
        table.deal_damage(4, iter::empty(), 10);
        table.deal_damage(3, iter::empty(), 40);

        assert_eq!(Some(3), table.target());
        table.remove(3);
        assert_eq!(Some(2), table.target());
        table.remove(2);
        assert_eq!(Some(1), table.target());
        table.remove(1);
        assert_eq!(Some(4), table.target());
        table.remove(4);
        assert_eq!(None, table.target());
    }

    #[test]
    fn test_priority_by_time_added() {
        let mut table: ThreatTable = HashMap::new().into();
        table.deal_damage(2, iter::empty(), 0);
        table.deal_damage(1, iter::empty(), 0);
        table.deal_damage(4, iter::empty(), 0);
        table.deal_damage(3, iter::empty(), 0);

        assert_eq!(Some(2), table.target());
        table.remove(1);
        assert_eq!(Some(2), table.target());
        table.remove(2);
        assert_eq!(Some(4), table.target());
        table.remove(4);
        assert_eq!(Some(3), table.target());
        table.remove(3);
        assert_eq!(None, table.target());
    }

    #[test]
    fn test_retain() {
        let mut table: ThreatTable = HashMap::from_iter(vec![
            ("one".to_string(), 1),
            ("two".to_string(), 2),
            ("three".to_string(), 3),
            ("four".to_string(), 4),
        ])
        .into();
        table.deal_damage(2, ["two".to_string()].iter(), 30);
        table.deal_damage(1, ["one".to_string()].iter(), 20);
        table.deal_damage(4, ["four".to_string()].iter(), 10);
        table.deal_damage(3, ["three".to_string()].iter(), 40);

        table.retain(|guid, _| *guid % 2 == 0);

        assert_eq!(Some(4), table.target());
        table.remove(4);
        assert_eq!(Some(2), table.target());
        table.remove(2);
        assert_eq!(None, table.target());
    }
}

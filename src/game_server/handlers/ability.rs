use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    path::Path,
};

use serde::Deserialize;

use crate::{game_server::packets::AbilitySubType, ConfigError};

const fn default_ability_sub_type() -> AbilitySubType {
    AbilitySubType::InstantSingleTarget
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilitySlotConfig {
    pub icon_set_id: u32,
    pub name_id: u32,
    #[serde(default)]
    pub icon_tint_id: u32,
    #[serde(default)]
    pub required_force_points: u32,
    #[serde(default)]
    pub use_cooldown_millis: u32,
    #[serde(default)]
    pub init_cooldown_millis: u32,
    #[serde(default)]
    pub area_of_effect_radius: f32,
    #[serde(default)]
    pub max_distance_from_player: f32,
    #[serde(default = "default_ability_sub_type")]
    pub ability_sub_type: AbilitySubType,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityConfig {
    pub ability_key: String,
    pub slot_info: AbilitySlotConfig,
}

pub struct PlayerAbility {
    #[allow(dead_code)]
    pub ability_id: u32,
    pub slot_info: AbilitySlotConfig,
}

impl PlayerAbility {
    pub fn from_config(config: AbilityConfig, id: u32) -> Self {
        Self {
            ability_id: id,
            slot_info: config.slot_info,
        }
    }
}

pub fn load_abilities(
    config_dir: &Path,
) -> Result<(BTreeMap<u32, PlayerAbility>, HashMap<String, u32>), ConfigError> {
    let file = File::open(config_dir.join("abilities.yaml"))?;
    let ability_configs: Vec<AbilityConfig> = serde_yaml::from_reader(file)?;

    let mut abilities = BTreeMap::new();
    let ability_count = ability_configs.len();
    let mut keys_to_id = HashMap::with_capacity(ability_count);

    for (index, config) in ability_configs.into_iter().enumerate() {
        let id = (index + 1) as u32;
        if keys_to_id.insert(config.ability_key.clone(), id).is_some() {
            return Err(ConfigError::ConstraintViolated(format!(
                "Duplicate (Ability Key: {}) found in abilities.yaml",
                config.ability_key
            )));
        }

        abilities.insert(id, PlayerAbility::from_config(config, id));
    }

    Ok((abilities, keys_to_id))
}

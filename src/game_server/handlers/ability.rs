use std::{collections::HashMap, fs::File, path::Path};

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

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityConfig {
    pub slot_info: AbilitySlotConfig,
}

pub fn load_abilities(config_dir: &Path) -> Result<HashMap<String, AbilityConfig>, ConfigError> {
    let file = File::open(config_dir.join("abilities.yaml"))?;
    let abilities: HashMap<String, AbilityConfig> = serde_yaml::from_reader(file)?;

    Ok(abilities)
}

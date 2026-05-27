use std::{
    collections::HashMap,
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

use packet_serialize::DeserializePacket;
use serde::Deserialize;

use crate::{
    game_server::{
        packets::{ability::AbilityOpCode, AbilitySubType},
        Broadcast, ProcessPacketError, ProcessPacketErrorType,
    },
    ConfigError,
};

const fn default_ability_sub_type() -> AbilitySubType {
    AbilitySubType::InstantSingleTarget
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityConfig {
    pub icon_set_id: u32,
    pub name_id: u32,
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

pub fn load_abilities(config_dir: &Path) -> Result<HashMap<String, AbilityConfig>, ConfigError> {
    let file = File::open(config_dir.join("abilities.yaml"))?;
    let abilities: HashMap<String, AbilityConfig> = serde_yaml::from_reader(file)?;

    Ok(abilities)
}

pub fn process_ability(cursor: &mut Cursor<&[u8]>) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u16 = DeserializePacket::deserialize(cursor)?;
    match AbilityOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            // Ability definitions are presumably unused, so ignore
            AbilityOpCode::RequestDefinition => Ok(Vec::new()),
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unimplemented ability packet: {op_code:?}, {buffer:x?}"),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown ability packet: {raw_op_code}, {buffer:x?}"),
            ))
        }
    }
}

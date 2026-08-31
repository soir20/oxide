use std::{
    collections::HashMap,
    fs::File,
    io::{Cursor, Error, ErrorKind, Read},
    path::Path,
};

use evalexpr::{context_map, eval_with_context, Value};
use packet_serialize::DeserializePacket;
use rand::{thread_rng, Rng};
use serde::Deserialize;

use crate::{
    game_server::{
        handlers::{character::CharacterStats, guid::Guid},
        packets::{ability::AbilityOpCode, player_update::HitPointModification, AbilitySubType},
        Broadcast, GamePacket, ProcessPacketError, ProcessPacketErrorType, TunneledPacket,
    },
    ConfigError, GameServer,
};

const DEFAULT_DAMAGE_EXPRESSION: &str = "x * random(0.84, 1.15)";

const fn default_base_damage() -> i16 {
    100
}

const fn default_critical_chance() -> u32 {
    5
}

const fn default_max_distance_from_player() -> f32 {
    15.0
}

const fn default_ability_sub_type() -> AbilitySubType {
    AbilitySubType::InstantSingleTarget
}

fn default_damage_expression() -> String {
    DEFAULT_DAMAGE_EXPRESSION.to_string()
}

fn evaluate_damage_expression(
    damage_expression: &str,
    damage: i16,
    ability_name: String,
) -> Result<i16, Error> {
    let context = context_map! {
        "x" => evalexpr::Value::Float(damage as f64),
    }
    .map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Couldn't build expression evaluation context for ability {ability_name}"),
        )
    })?;

    let result = eval_with_context(damage_expression, &context).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Unable to evaluate cost expression for ability {ability_name}: {err}"),
        )
    })?;

    let Value::Float(damage) = result else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Damage expression did not return an integer for ability {ability_name}, returned: {result}"
            ),
        ));
    };

    i16::try_from(damage.round() as i64).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "Damage expression returned float that could not be converted to an integer for ability {ability_name}: {damage}, {err}"
            ),
        )
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Default)]
pub enum TargetLimit {
    #[default]
    Single,
    Infinite,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Default)]
pub enum TargetType {
    #[default]
    Enemy,
    Friendly,
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
    #[serde(default = "default_max_distance_from_player")]
    pub max_distance_from_player: f32,
    #[serde(default = "default_base_damage")]
    pub base_damage: i16,
    #[serde(default = "default_damage_expression")]
    pub damage_expression: String,
    #[serde(default = "default_critical_chance")]
    pub critical_chance: u32,
    pub critical_bonus_percent: Option<u32>,
    #[serde(default)]
    pub target_limit: TargetLimit,
    #[serde(default)]
    pub target_type: TargetType,
    pub cast_animation_id: Option<u32>,
    pub cast_composite_effect_id: Option<u32>,
    pub cast_composite_effect_seconds: Option<f32>,
    pub impact_animation_id: Option<u32>,
    pub impact_composite_effect_id: Option<u32>,
    #[serde(default)]
    pub target_bone_name: String,
    #[serde(default = "default_ability_sub_type")]
    pub ability_sub_type: AbilitySubType,
}

pub fn load_abilities(config_dir: &Path) -> Result<HashMap<String, AbilityConfig>, ConfigError> {
    let file = File::open(config_dir.join("abilities.yaml"))?;
    let abilities: HashMap<String, AbilityConfig> = serde_yaml::from_reader(file)?;

    Ok(abilities)
}

fn compute_ability_damage(
    config: &AbilityConfig,
    ability_name: String,
) -> Result<(i16, bool), Error> {
    let evaluated_damage =
        evaluate_damage_expression(&config.damage_expression, config.base_damage, ability_name)?;

    let mut rng = thread_rng();
    let is_critical = rng.gen_range(0..100) < config.critical_chance;

    let final_damage = if is_critical {
        let bonus_percent = config.critical_bonus_percent.unwrap_or(0);
        let multiplier = 1.0 + (bonus_percent as f64 / 100.0);

        let crit_damage = (evaluated_damage as f64) * multiplier;
        i16::try_from(crit_damage.round() as i64).unwrap_or(evaluated_damage)
    } else {
        evaluated_damage
    };

    Ok((final_damage, is_critical))
}

fn deal_ability_damage(
    caster: u64,
    target: &mut CharacterStats,
    nearby_player_guids: &[u32],
    ability_config: &AbilityConfig,
    ability_name: String,
) -> Result<Vec<Broadcast>, Error> {
    let (damage_dealt, critical) = compute_ability_damage(ability_config, ability_name)?;
    let damaged = damage_dealt > 0;
    let current_health = target.health as i32;
    let max_health = target.max_health as i32;

    let new_health = (current_health - damage_dealt as i32).clamp(0, max_health) as u16;
    let hp_delta = (new_health as i32) - current_health;

    target.health = new_health;

    let mut broadcasts = vec![Broadcast::Multi(
        nearby_player_guids.to_vec(),
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: HitPointModification {
                attacker_guid: caster,
                receiver_guid: Guid::guid(target),
                show_hp_delta: true,
                max_hp: target.max_health as i32,
                new_hp: new_health as i32,
                hp_delta,
                critical,
            },
        })],
    )];

    if damaged && new_health == 0 {
        broadcasts.extend(target.knock_out(nearby_player_guids));
    }

    Ok(broadcasts)
}

pub fn process_ability(
    game_server: &GameServer,
    sender: u32,
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
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

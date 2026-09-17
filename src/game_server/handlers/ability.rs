use std::{
    collections::{BTreeMap, HashMap},
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
        packets::{
            ability::{AbilityOpCode, AbilityTargetType, CastAndLand, RequestStartCast},
            player_update::{HitPointModification, UpdateWieldType},
            AbilitySubType, ActionBarType, CharacterBoneNameTarget, Pos, Target,
        },
        Broadcast, GamePacket, ProcessPacketError, ProcessPacketErrorType, TunneledPacket,
    },
    ConfigError, GameServer,
};

use super::{
    character::{coerce_to_broadcast_supplier, CharacterStats, CharacterType},
    combat::player_can_attack,
    distance3_pos,
    guid::{Guid, GuidTableIndexer, IndexedGuid},
    lock_enforcer::{CharacterLockRequest, CharacterWriteGuard, ZoneLockEnforcer, ZoneLockRequest},
    unique_guid::player_guid,
    zone::ZoneInstance,
    WriteLockingBroadcastSupplier,
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
    ability_key: &str,
) -> Result<i16, Error> {
    let context = context_map! {
        "x" => evalexpr::Value::Float(damage as f64),
    }
    .unwrap_or_else(|_| {
        panic!("Couldn't build expression evaluation context for ability {ability_key}")
    });

    let result = eval_with_context(damage_expression, &context).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Unable to evaluate damage expression for ability {ability_key}: {err}"),
        )
    })?;

    let Value::Float(damage) = result else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Damage expression did not return an integer for ability {ability_key}, returned: {result}"
            ),
        ));
    };

    i16::try_from(damage.round() as i64).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "Damage expression returned float that could not be converted to an integer for ability {ability_key}: {damage}, {err}"
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

fn compute_ability_damage(config: &AbilityConfig, ability_key: &str) -> Result<(i16, bool), Error> {
    let evaluated_damage =
        evaluate_damage_expression(&config.damage_expression, config.base_damage, ability_key)?;

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
    target_stats: &mut CharacterStats,
    nearby_player_guids: &[u32],
    ability_key: &str,
    ability_config: &AbilityConfig,
) -> Result<Vec<Broadcast>, Error> {
    let (damage_dealt, critical) = compute_ability_damage(ability_config, ability_key)?;
    let damaged = damage_dealt > 0;
    let current_health = target_stats.health as i32;
    let max_health = target_stats.max_health as i32;

    let new_health = (current_health - damage_dealt as i32).clamp(0, max_health) as u16;
    let hp_delta = (new_health as i32) - current_health;

    target_stats.health = new_health;

    let mut broadcasts = vec![Broadcast::Multi(
        nearby_player_guids.to_vec(),
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: HitPointModification {
                attacker_guid: caster,
                receiver_guid: Guid::guid(target_stats),
                show_hp_delta: true,
                max_hp: target_stats.max_health as i32,
                new_hp: new_health as i32,
                hp_delta,
                critical,
            },
        })],
    )];

    if damaged && new_health == 0 {
        broadcasts.extend(target_stats.knock_out(nearby_player_guids));
    }

    Ok(broadcasts)
}

fn make_cast_and_land_packet(
    caster: u64,
    targets: &[u64],
    ability_config: &AbilityConfig,
    action_bar_type: ActionBarType,
    ability_slot_index: i32,
) -> Vec<Vec<u8>> {
    vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: CastAndLand {
            caster_guid: caster,
            targets: targets
                .iter()
                .map(|&target| {
                    Target::CharacterBone(CharacterBoneNameTarget {
                        fallback_pos: Pos::default(),
                        character_guid: target,
                        bone_name: ability_config.target_bone_name.clone(),
                    })
                })
                .collect(),
            unknown1: -1,
            unknown2: 0,
            cast_animation_id: ability_config.cast_animation_id.unwrap_or(0),
            cast_composite_effect_id: ability_config.cast_composite_effect_id.unwrap_or(0),
            slot_cooldown_millis: ability_config.use_cooldown_millis,
            disable_slot_cooldown: false,
            unknown7: false,
            impact_animation_id: ability_config.impact_animation_id.unwrap_or(0),
            impact_composite_effect_id1: ability_config.impact_composite_effect_id.unwrap_or(0),
            unknown10: 0,
            unknown11: Pos::default(),
            cast_composite_effect_seconds: ability_config
                .cast_composite_effect_seconds
                .unwrap_or(0.0),
            unknown13: 0.0,
            unknown14: 0,
            action_bar_type,
            slot_index: ability_slot_index,
            unknown17: 0,
            override_launcher_guid: 0,
            unknown19: false,
            unknown20: 0,
            unknown21: 0,
            projectile_start_speed: 60.0,
            projectile_end_speed: 60.0,
            unknown24: 0,
            unknown25: 0,
            unknown26: Pos::default(),
            unknown27: Pos::default(),
            projectile_adr_name: "".to_string(),
            projectile_origin: Target::CharacterBone(CharacterBoneNameTarget {
                fallback_pos: Pos::default(),
                character_guid: caster,
                bone_name: "".to_string(),
            }),
            unknown_target: Target::default(),
            unknown29: Pos::default(),
            projectile_angular_speed: 0.0,
            unknown31: false,
            projectile_start_size: 1.0,
            projectile_end_size: 1.0,
            projectile_trail_composite_effect_id: 0,
            impact_composite_effect_id2: ability_config.impact_composite_effect_id.unwrap_or(0),
            unknown36: 0,
            unknown37: 0,
            unknown38: 0.0,
            unknown39: 0.0,
            unknown40: 0.0,
            unknown41: 0.0,
            unknown42: 0.0,
            unknown43: 0.0,
            unknown44: 0.0,
            missfire_travel_units: 20.0,
            unknown46: "".to_string(),
            unknown47: 0,
        },
    })]
}

pub fn handle_targeted_cast(
    caster_stats: &mut CharacterStats,
    target_guid: u64,
    nearby_characters: &mut BTreeMap<u64, CharacterWriteGuard>,
    nearby_player_guids: &[u32],
    ability_name: &str,
    ability_config: &AbilityConfig,
    action_bar_type: ActionBarType,
    slot_index: i32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let caster_guid = Guid::guid(caster_stats);

    let all_targets_nearby: Vec<_> = nearby_characters
        .iter()
        .filter(|(_, target)| {
            player_can_attack(
                &target.stats,
                caster_stats,
                &game_server.enemy_types.allowed_player_attacks,
            )
        })
        .map(|(&id, _)| id)
        .collect();

    let valid_targets: Vec<_> = all_targets_nearby
        .iter()
        .filter_map(|&id| {
            let distance = distance3_pos(caster_stats.pos, nearby_characters[&id].stats.pos);

            (distance <= ability_config.max_distance_from_player).then_some((id, distance))
        })
        .collect();

    let selected_targets = match ability_config.target_limit {
        TargetLimit::Infinite => valid_targets.into_iter().map(|(id, _)| id).collect(),
        TargetLimit::Single => {
            if valid_targets.iter().any(|&(id, _)| id == target_guid) {
                vec![target_guid]
            } else {
                valid_targets
                    .into_iter()
                    .min_by(|(_, distance_a), (_, distance_b)| distance_a.total_cmp(distance_b))
                    .map(|(id, _)| id)
                    .into_iter()
                    .collect()
            }
        }
    };

    let mut broadcasts = vec![Broadcast::Multi(
        nearby_player_guids.to_vec(),
        make_cast_and_land_packet(
            caster_guid,
            &selected_targets,
            ability_config,
            action_bar_type,
            slot_index,
        ),
    )];

    for &target in &selected_targets {
        if let Some(target_write_handle) = nearby_characters.get_mut(&target) {
            broadcasts.extend(deal_ability_damage(
                caster_guid,
                &mut target_write_handle.stats,
                nearby_player_guids,
                ability_name,
                ability_config,
            )?);
        }
    }

    let aoe_radius = ability_config.area_of_effect_radius;

    if aoe_radius > 0.0 {
        for &selected_target in &selected_targets {
            let Some(impacted_pos) = nearby_characters
                .get(&selected_target)
                .map(|target| target.stats.pos)
            else {
                continue;
            };

            for &aoe_target in &all_targets_nearby {
                let Some(target_write_handle) = nearby_characters.get_mut(&aoe_target) else {
                    continue;
                };

                if distance3_pos(impacted_pos, target_write_handle.stats.pos) <= aoe_radius {
                    broadcasts.extend(deal_ability_damage(
                        caster_guid,
                        &mut target_write_handle.stats,
                        nearby_player_guids,
                        ability_name,
                        ability_config,
                    )?);
                }
            }
        }
    }

    Ok(broadcasts)
}

fn process_start_cast(
    caster: u64,
    cast_req: RequestStartCast,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let broadcast_supplier: WriteLockingBroadcastSupplier = game_server
        .lock_enforcer()
        .read_characters(|characters_table_read_handle| {
            let mut write_guids = vec![caster];

            if let Some((_, instance_guid, chunk)) = characters_table_read_handle.index1(caster) {
                write_guids.extend(ZoneInstance::all_characters_nearby(
                    chunk,
                    instance_guid,
                    characters_table_read_handle,
                ));
            }

            CharacterLockRequest {
                read_guids: Vec::new(),
                write_guids,
                character_consumer: move |characters_table_read_handle, _, mut characters_write, minigame_data_lock_enforcer| {
                    let Some(mut caster_write_handle) = characters_write.remove(&caster) else {
                        return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                    };

                    let caster_instance = caster_write_handle.stats.instance_guid;
                    let caster_chunk = caster_write_handle.index1().2;

                    let nearby_player_guids = ZoneInstance::all_players_nearby(
                        caster_chunk,
                        caster_instance,
                        characters_table_read_handle,
                    );

                    let zones_lock_enforcer: ZoneLockEnforcer = minigame_data_lock_enforcer.into();

                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                        read_guids: vec![caster_instance],
                        write_guids: Vec::new(),
                        zone_consumer: move |_, zones_read, _| {
                            if !zones_read.contains_key(&caster_instance) {
                                return coerce_to_broadcast_supplier(move |_| {
                                    Err(ProcessPacketError::new(
                                        ProcessPacketErrorType::ConstraintViolated,
                                        format!(
                                            "Caster {caster} is in a non-existent zone {caster_instance}"
                                        ),
                                    ))
                                });
                            }

                            let result = (|| {
                                let player_stats =
                                    match &mut caster_write_handle.stats.character_type {
                                        CharacterType::Player(player) => player.as_mut(),
                                        _ => {
                                            return Err(ProcessPacketError::new(
                                                ProcessPacketErrorType::ConstraintViolated,
                                                format!(
                                                    "Received request from {caster} to cast an ability but they were not a player"
                                                ),
                                            ));
                                        }
                                    };

                                let slot_index = cast_req.slot_index as usize;

                                let Some(ability_key) = player_stats
                                    .action_bar
                                    .weapon_abilities
                                    .iter()
                                    .flat_map(|group| group.ability_keys.iter())
                                    .nth(slot_index)
                                    .cloned()
                                else {
                                    return Err(ProcessPacketError::new(
                                        ProcessPacketErrorType::ConstraintViolated,
                                        format!(
                                            "Caster {caster} attempted to cast slot index {slot_index} but no abilities were found"
                                        ),
                                    ));
                                };

                                let Some(ability_config) = game_server.abilities.get(&ability_key) else {
                                    return Err(ProcessPacketError::new(
                                        ProcessPacketErrorType::ConstraintViolated,
                                        format!(
                                            "Caster {caster} attempted to cast unknown ability {ability_key}"
                                        ),
                                    ));
                                };

                                let mut broadcasts = Vec::new();

                                if !caster_write_handle.is_brandished() {
                                    caster_write_handle.brandish_or_holster();
                                    broadcasts.push(Broadcast::Multi(
                                        nearby_player_guids.clone(),
                                        vec![
                                            GamePacket::serialize(&TunneledPacket {
                                                unknown1: true,
                                                inner: UpdateWieldType {
                                                    guid: caster,
                                                    wield_type: caster_write_handle.stats.wield_type(),
                                                },
                                            }),
                                        ],
                                    ));
                                }

                                match &cast_req.target {
                                    AbilityTargetType::Guid(guid_target) => {
                                        broadcasts.extend(handle_targeted_cast(
                                            &mut caster_write_handle.stats,
                                            guid_target.target_guid2,
                                            &mut characters_write,
                                            &nearby_player_guids,
                                            &ability_key,
                                            ability_config,
                                            cast_req.action_bar_type,
                                            slot_index as i32,
                                            &game_server,
                                        )?);
                                    }
                                    AbilityTargetType::WithSelf(_) | AbilityTargetType::Aoe(_) => {}
                                }

                                Ok(broadcasts)
                            })();

                            characters_write.insert(caster, caster_write_handle);
                            coerce_to_broadcast_supplier(move |_| result)
                        },
                    })
                },
            }
        });

    broadcast_supplier?(game_server)
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
            AbilityOpCode::RequestStartCast => {
                let cast_req = RequestStartCast::deserialize(cursor)?;
                process_start_cast(player_guid(sender), cast_req, game_server)
            }
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

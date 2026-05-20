use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::{
        character::{MinigameStatus, Player},
        inventory::player_has_saber_equipped,
    },
    packets::{
        attack_cruiser::{
            AttackCruiserAddActor, AttackCruiserAddPlayer, AttackCruiserAnyConfig,
            AttackCruiserClientConfig, AttackCruiserConfig, AttackCruiserConfigType,
            AttackCruiserGameConfig, AttackCruiserGlobalConfig, AttackCruiserHudMessageConfig,
            AttackCruiserOpCode, AttackCruiserPlanetConfig, AttackCruiserPlayerUpdate,
            AttackCruiserPlayerUpdateUnknown1, AttackCruiserPlayerUpdateUnknown2,
            AttackCruiserUpdateGameState,
        },
        minigame::{MinigameHeader, ScoreEntry, ScoreType},
        saber_strike::{
            SaberStrikeGameOver, SaberStrikeObfuscatedScore, SaberStrikeOpCode,
            SaberStrikeSingleKill, SaberStrikeStageData, SaberStrikeThrowKill,
        },
        tunnel::TunneledPacket,
        ui::ExecuteScriptWithStringParams,
        GamePacket, Pos3,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::minigame::{handle_minigame_packet_write, MinigameTypeData};

pub fn start_saber_strike(
    saber_strike_stage_id: u32,
    player: &Player,
    minigame_status: &MinigameStatus,
    game_server: &GameServer,
) -> Vec<Vec<u8>> {
    vec![
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserClientConfig {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::ClientConfig as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                config1: AttackCruiserConfig {
                    unknown1: 0,
                    unknown2: 0x4c61446a,
                    unknown3: "1234567890123456789012345678901234567890".to_string(),
                    config_type: AttackCruiserConfigType::Game(AttackCruiserGameConfig {
                        id: 27001,
                        encounter_id: 0,
                        sound_id: 0,
                        mode: 1,
                        global_config: AttackCruiserGlobalConfig {
                            physics_speed: 5.0,
                            connect_timeout_seconds: 5.0,
                            ready_timeout_seconds: 15.0,
                            default_timeout_seconds: 25.0,
                            effects_preload_timeout_seconds: 35.0,
                            effects_ready_timeout_seconds: 45.0,
                            server_update_players_interval_seconds: 5.0,
                            server_update_actors_interval_seconds: 5.0,
                            server_draw_debug_data_interval_seconds: 5.0,
                            client_update_actors_interval_seconds: 5.0,
                            max_interpolation_step: 5.0,
                            small_mass_threshold: 15.0,
                            dodge_prediction_time: 5.0,
                            dodge_separation: 5.0,
                            player_perfect_aim_radius: 5.0,
                            player_auto_aim_assistance: 5.0,
                            npc_auto_aim_assistance: 5.0,
                            player_blaster_trapezoid_width: 5.0,
                            player_auto_aim_range: 5.0,
                            npc_auto_aim_range: 5.0,
                            player_blaster_vertical_range: 5.0,
                            npc_blaster_vertical_range: 5.0,
                            min_blaster_speed: 5.0,
                            max_blaster_angle: 15.0,
                            projectile_ray_advance_seconds: 5.0,
                            projectile_ray_spacing: 5.0,
                            projectile_ray_iterations: 5,
                            advance_launch_seconds: 5.0,
                            advance_interception_time: 5.0,
                            collisionless_time: 10,
                            tractionless_time: 20,
                            screen_relative_turning: false,
                            ship_to_ship_collision: true,
                            player_death_animation_delay_seconds: 5.0,
                            respawn_damage_area: 5.0,
                            respawn_delay_seconds: 5.0,
                            respawn_invulnerable_seconds: 5.0,
                            enable_composite_effects: true,
                            torpedo_reticule_effect_id: 123,
                            torpedo_reticule_effect_seconds: 5.0,
                            fighter_reticule_effect_id: 456,
                            fighter_reticule_effect_seconds: 5.0,
                            wave_end_sound_id: 1234,
                            damage_warning_sound_id: 5678,
                            damage_warning_interval_seconds: 5.0,
                            mine_deploy_sound_id: 1234,
                            fighter_launch_sound_id: 5678,
                            score_meter_tier1: 100,
                            score_decay_tier1: 10,
                            score_meter_exponent: 5.0,
                            score_decay_exponent: 5.0,
                            health_foreground_image_id: 1000,
                            health_background_image_id: 1001,
                            health_foreground_internal_id: 1002,
                            health_background_internal_id: 1003,
                            enable_weapon_tiers: true,
                            player_death_spawn_config: AttackCruiserAnyConfig {
                                class: "".to_string(),
                                value: "".to_string(),
                            },
                            hud_message: AttackCruiserHudMessageConfig {
                                speaker_name_id: 100,
                                speaker_image_id: 1000,
                                message_id: 101,
                                sound_id: 1234,
                                duration_millis: 10000,
                                delay_millis: 0,
                            },
                        },
                        end_condition_config: AttackCruiserAnyConfig {
                            class: "".to_string(),
                            value: "".to_string(),
                        },
                        win_condition_config: AttackCruiserAnyConfig {
                            class: "".to_string(),
                            value: "".to_string(),
                        },
                        target_value1: 0,
                        target_value2: 0,
                        playfield_height: 50.0,
                        playfield_length: 120.0,
                        playfield_width: 120.0,
                        playfield_warning_length: 130.0,
                        playfield_warning_width: 130.0,
                        playfield_center_x: 123.0,
                        playfield_center_z: 123.0,
                        kill_zone_height: 20.0,
                        enemy_attack_radius: 50.0,
                        endless_waves: false,
                        debugged_actors: 0,
                        planet_tilt_init_x: 0.0,
                        planet_tilt_init_z: 0.0,
                        planet_tilt_rate_x: 10.0,
                        planet_tilt_rate_z: 20.,
                        planet: AttackCruiserPlanetConfig {
                            model_id: 583,
                            pos: Pos3::default(),
                            rotation_speed: 5.0,
                        },
                        players: Vec::new(),
                        events: Vec::new(),
                        actor_pools: Vec::new(),
                        waves: Vec::new(),
                    }),
                },
                config2: AttackCruiserConfig {
                    unknown1: 1,
                    unknown2: 0x79243a4c,
                    unknown3: "testing".to_string(),
                    config_type: AttackCruiserConfigType::Global {},
                },
                config3: AttackCruiserConfig {
                    unknown1: 2,
                    unknown2: 0x79243a4c,
                    unknown3: "".to_string(),
                    config_type: AttackCruiserConfigType::Global {},
                },
                configs: Vec::new(),
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserAddActor {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::AddActor as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                actor_id: 500,
                unknown2: 2,
                actor_pool_id: 1000,
                unknown4: Pos3::default(),
                unknown5: Pos3::default(),
                unknown6: 4,
                unknown7: 5,
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserAddPlayer {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::AddPlayer as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                guid: 1,
                update: AttackCruiserPlayerUpdate {
                    unknown1: Some(AttackCruiserPlayerUpdateUnknown1 {
                        unknown1: 1,
                        actor_id: 500,
                        unknown3: 200,
                        unknown4: "test".to_string(),
                        unknown5: "hello world".to_string(),
                    }),
                    unknown2: Some(AttackCruiserPlayerUpdateUnknown2 {
                        unknown1: 10,
                        unknown2: 11,
                        unknown3: 12,
                        unknown4: 13,
                        unknown5: 14,
                        unknown6: 15,
                    }),
                    unknown3: None,
                    unknown4: None,
                    unknown5: None,
                },
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserUpdateGameState {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::UpdateGameState as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                game_state: 4,
            },
        }),
    ]
}

pub fn process_saber_strike_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let header = MinigameHeader::deserialize(cursor)?;
    match SaberStrikeOpCode::try_from(header.sub_op_code) {
        Ok(op_code) => match op_code {
            SaberStrikeOpCode::GameOver => {
                let game_over = SaberStrikeGameOver::deserialize(cursor)?;
                handle_saber_strike_game_over(&header, &game_over, sender, game_server)
            }
            SaberStrikeOpCode::SingleKill => {
                let _ = SaberStrikeSingleKill::deserialize(cursor)?;
                // TODO: update player achievement progress
                Ok(Vec::new())
            }
            SaberStrikeOpCode::ThrowKill => {
                let _ = SaberStrikeThrowKill::deserialize(cursor)?;
                // TODO: update player achievement progress
                Ok(Vec::new())
            }
            SaberStrikeOpCode::ObfuscatedScore => {
                let obfuscated_score_packet = SaberStrikeObfuscatedScore::deserialize(cursor)?;
                handle_minigame_packet_write(
                    sender,
                    game_server,
                    &header,
                    |minigame_status, _, _, _, _, _| {
                        match &mut minigame_status.type_data {
                            MinigameTypeData::SaberStrike { obfuscated_score } => {
                                *obfuscated_score = obfuscated_score_packet.score();
                                Ok(Vec::new())
                            },
                            _ => Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!("Player {sender} sent a Saber Strike obfuscated score packet, but they have no Saber Strike game data")
                            ))
                        }
                    },
                )
            }
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unimplemented minigame op code: {op_code:?} {buffer:x?}"),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!(
                    "Unknown minigame packet: {}, {buffer:x?}",
                    header.sub_op_code
                ),
            ))
        }
    }
}

fn handle_saber_strike_game_over(
    header: &MinigameHeader,
    game_over: &SaberStrikeGameOver,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    handle_minigame_packet_write(
        sender,
        game_server,
        header,
        |minigame_status, _, _, _, _, _| {
            let MinigameTypeData::SaberStrike { obfuscated_score } = minigame_status.type_data
            else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Player {sender} sent a Saber Strike game over packet, but they have no Saber Strike game data")
                ));
            };

            if obfuscated_score != game_over.total_score {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} sent a Saber Strike game over packet with score {}, but their obfuscated score was {obfuscated_score}",
                        game_over.total_score,
                    )
                ));
            }

            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_TotalTime".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Time,
                score_count: game_over.duration_seconds.round() as i32,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_ThrowsRemaining".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: game_over.remaining_sabers,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_TotalDestroyed".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: game_over.enemies_killed,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_BestThrow".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: game_over.best_throw,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_TotalScore".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Total,
                score_count: game_over.total_score,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.total_score = game_over.total_score;
            minigame_status.win_status.set_won(game_over.won);
            Ok(vec![Broadcast::Single(
                sender,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: ExecuteScriptWithStringParams {
                        script_name: "Ui.QuitMiniGame".to_string(),
                        params: Vec::new(),
                    },
                })],
            )])
        },
    )
}

use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::{
        character::{MinigameStatus, Player},
        inventory::player_has_saber_equipped,
    },
    packets::{
        attack_cruiser::{
            AttackCruiserActorPoolConfig, AttackCruiserAddActor, AttackCruiserAddPlayer,
            AttackCruiserAnyConfig, AttackCruiserBool, AttackCruiserCameraConfig,
            AttackCruiserClientConfig, AttackCruiserConfig, AttackCruiserConfigType,
            AttackCruiserGameConfig, AttackCruiserGameWaveConfig, AttackCruiserGlobalConfig,
            AttackCruiserHudMessageConfig, AttackCruiserOpCode, AttackCruiserPlanetConfig,
            AttackCruiserPlayerConfig, AttackCruiserPlayerUpdate,
            AttackCruiserPlayerUpdateUnknown1, AttackCruiserPlayerUpdateUnknown2,
            AttackCruiserUpdateGameState, AttackCruiserVec, AttackCruiserWaveConfig,
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
                    unknown1: 1,
                    unknown2: 0x79243a4c,
                    unknown3: "configtest".to_string(),
                    config_type: AttackCruiserConfigType::Global(Box::new(
                        AttackCruiserGlobalConfig {
                            physics_speed: 1.0,
                            connect_timeout_seconds: 1.0,
                            ready_timeout_seconds: 1.0,
                            default_timeout_seconds: 1.0,
                            effects_preload_timeout_seconds: 1.0,
                            effects_ready_timeout_seconds: 1.0,
                            server_update_players_interval_seconds: 1.0,
                            server_update_actors_interval_seconds: 1.0,
                            server_draw_debug_data_interval_seconds: 1.0,
                            client_update_actors_interval_seconds: 1.0,
                            max_interpolation_step: 1.0,
                            small_mass_threshold: 1.0,
                            dodge_prediction_time: 1.0,
                            dodge_separation: 1.0,
                            player_perfect_aim_radius: 1.0,
                            player_auto_aim_assistance: 1.0,
                            npc_auto_aim_assistance: 1.0,
                            player_blaster_trapezoid_width: 1.0,
                            player_auto_aim_range: 1.0,
                            npc_auto_aim_range: 1.0,
                            player_blaster_vertical_range: 1.0,
                            npc_blaster_vertical_range: 1.0,
                            min_blaster_speed: 1.0,
                            max_blaster_angle: 1.0,
                            projectile_ray_advance_seconds: 1.0,
                            projectile_ray_spacing: 1.0,
                            projectile_ray_iterations: 10,
                            advance_launch_seconds: 1.0,
                            advance_interception_time: 1.0,
                            collisionless_time: 2000,
                            tractionless_time: 5000,
                            screen_relative_turning: AttackCruiserBool(false),
                            ship_to_ship_collision: AttackCruiserBool(true),
                            player_death_animation_delay_seconds: 1.0,
                            respawn_damage_area: 1.0,
                            respawn_delay_seconds: 1.0,
                            respawn_invulnerable_seconds: 1.0,
                            enable_composite_effects: AttackCruiserBool(true),
                            torpedo_reticule_effect_id: 1234,
                            torpedo_reticule_effect_seconds: 1.0,
                            fighter_reticule_effect_id: 5678,
                            fighter_reticule_effect_seconds: 1.0,
                            wave_end_sound_id: 1234,
                            damage_warning_sound_id: 5678,
                            damage_warning_interval_seconds: 1.0,
                            mine_deploy_sound_id: 1234,
                            fighter_launch_sound_id: 5678,
                            score_meter_tier1: 1000,
                            score_decay_tier1: 10,
                            score_meter_exponent: 1.0,
                            score_decay_exponent: 1.0,
                            health_foreground_image_id: 100,
                            health_background_image_id: 200,
                            health_foreground_internal_id: 300,
                            health_background_internal_id: 400,
                            enable_weapon_tiers: AttackCruiserBool(true),
                            player_death_spawn_config: AttackCruiserAnyConfig {
                                class: "player death spawn config class".to_string(),
                                value: "player death spawn config value".to_string(),
                            },
                            hud_message: AttackCruiserHudMessageConfig {
                                speaker_name_id: 100,
                                speaker_image_id: 1000,
                                message_id: 200,
                                sound_id: 1234,
                                duration_millis: 10000,
                                delay_millis: 0,
                            },
                        },
                    )),
                },
                config2: AttackCruiserConfig {
                    unknown1: 2,
                    unknown2: 0x4c61446a,
                    unknown3: "GameConfig".to_string(),
                    config_type: AttackCruiserConfigType::Game(Box::new(AttackCruiserGameConfig {
                        id: 27001,
                        encounter_id: 0,
                        sound_id: 2413,
                        mode: 1,
                        global_config: AttackCruiserAnyConfig {
                            class: "global".to_string(),
                            value: "configtest".to_string(),
                        },
                        end_condition_config: AttackCruiserAnyConfig {
                            class: "".to_string(),
                            value: "".to_string(),
                        },
                        win_condition_config: AttackCruiserAnyConfig {
                            class: "".to_string(),
                            value: "".to_string(),
                        },
                        target_value1: 999,
                        target_value2: 888,
                        playfield_height: 30.0,
                        playfield_length: 1500.0,
                        playfield_width: 1500.0,
                        playfield_warning_length: 1500.0,
                        playfield_warning_width: 1500.0,
                        playfield_center_x: 0.0,
                        playfield_center_z: 0.0,
                        kill_zone_height: 20.0,
                        enemy_attack_radius: 50.0,
                        endless_waves: AttackCruiserBool(false),
                        debugged_actors: 5,
                        planet_tilt_init_x: 0.0,
                        planet_tilt_init_z: 0.0,
                        planet_tilt_rate_x: 10.0,
                        planet_tilt_rate_z: 20.0,
                        planet: AttackCruiserPlanetConfig {
                            model_id: 583,
                            pos: Pos3::default(),
                            rotation_speed: 5.0,
                        },
                        players: AttackCruiserVec(vec![AttackCruiserPlayerConfig {
                            ship_config: AttackCruiserAnyConfig {
                                class: "ship config class".to_string(),
                                value: "ship config value".to_string(),
                            },
                            camera_config: AttackCruiserAnyConfig {
                                class: "camera config class".to_string(),
                                value: "camera config value".to_string(),
                            },
                            lives: 5,
                            spawn_pos: Pos3 {
                                x: 120.0,
                                y: 120.0,
                                z: 120.0,
                            },
                            spawn_heading: 0.0,
                        }]),
                        events: AttackCruiserVec::new(),
                        actor_pools: AttackCruiserVec(vec![AttackCruiserActorPoolConfig {
                            actor_config: AttackCruiserAnyConfig {
                                class: "actor config class".to_string(),
                                value: "actor config value".to_string(),
                            },
                            size: 500,
                        }]),
                        waves: AttackCruiserVec(vec![AttackCruiserGameWaveConfig {
                            wave_config: AttackCruiserAnyConfig {
                                class: "hello".to_string(),
                                value: "world".to_string(),
                            },
                            launch_condition_config: AttackCruiserAnyConfig {
                                class: "blaster".to_string(),
                                value: "niceshot".to_string(),
                            },
                            complete_condition_config: AttackCruiserAnyConfig {
                                class: "".to_string(),
                                value: "".to_string(),
                            },
                            remove_actors_on_completion: AttackCruiserBool(false),
                        }]),
                    })),
                },
                config3: AttackCruiserConfig {
                    unknown1: 3,
                    unknown2: 0x6dc7e02b,
                    unknown3: "".to_string(),
                    config_type: AttackCruiserConfigType::Camera(Box::new(
                        AttackCruiserCameraConfig {
                            distance: 100.0,
                            min_distance: 0.0,
                            max_distance: 10000.0,
                            pitch: 30.0,
                            min_pitch: 0.0,
                            max_pitch: 100.0,
                            z_offset: 0.0,
                            target_tracking_hlq: 1.0,
                            zoom_step_q: 1.0,
                            zoom_step_hlq: 1.0,
                            forward_tether: AttackCruiserBool(false),
                            forward_tether_seconds: 1.0,
                            near_clip_distance: 1.0,
                            particle_update_distance: 1.0,
                            actor_update_radius: 1.0,
                            shadow_quality: 20,
                            shadow_draw_distance: 30000.0,
                            shadow_blob_render_distance: 20000.0,
                            overhead_render_distance: 10000.0,
                        },
                    )),
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
        // GamePacket::serialize(&TunneledPacket {
        //     unknown1: true,
        //     inner: AttackCruiserUpdateGameState {
        //         minigame_header: MinigameHeader {
        //             stage_guid: minigame_status.group.stage_guid,
        //             sub_op_code: AttackCruiserOpCode::UpdateGameState as i32,
        //             stage_group_guid: minigame_status.group.stage_group_guid,
        //         },
        //         game_state: 4,
        //     },
        // }),
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

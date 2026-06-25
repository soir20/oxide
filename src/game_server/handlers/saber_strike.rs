use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::character::{MinigameStatus, Player},
    packets::{
        attack_cruiser::{
            AttackCruiserActorConfig, AttackCruiserActorDamageStateConfig,
            AttackCruiserActorPoolConfig, AttackCruiserAddActor, AttackCruiserAddPlayer,
            AttackCruiserBasePhysicsConfig, AttackCruiserBlasterConfig, AttackCruiserBool,
            AttackCruiserCameraConfig, AttackCruiserClientConfig,
            AttackCruiserComplexPhysicsConfig, AttackCruiserComplexPhysicsGear,
            AttackCruiserConfig, AttackCruiserConfigDefinition, AttackCruiserConfigHash,
            AttackCruiserConfigName, AttackCruiserConfigPlayer, AttackCruiserConfigReference,
            AttackCruiserConfigType, AttackCruiserDeathSpawnConfig, AttackCruiserEventActorConfig,
            AttackCruiserEventCinematicConfig, AttackCruiserEventConfig, AttackCruiserGameConfig,
            AttackCruiserGameWaveConfig, AttackCruiserGlobalConfig, AttackCruiserHudMessageConfig,
            AttackCruiserOpCode, AttackCruiserPlanetConfig, AttackCruiserPlayerConfig,
            AttackCruiserPlayerState, AttackCruiserPlayerStateUnknown1, AttackCruiserShipConfig,
            AttackCruiserShipWeaponConfig, AttackCruiserSimplePhysicsConfig,
            AttackCruiserSimplePhysicsFlightConfig, AttackCruiserSpawnConfig,
            AttackCruiserUpdateGameState, AttackCruiserVec, AttackCruiserWaveConfig,
            AttackCruiserWeaponBayConfig,
        },
        minigame::{MinigameHeader, ScoreEntry, ScoreType},
        saber_strike::{
            SaberStrikeGameOver, SaberStrikeObfuscatedScore, SaberStrikeOpCode,
            SaberStrikeSingleKill, SaberStrikeThrowKill,
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
                    name: AttackCruiserConfigName {
                        hash: AttackCruiserConfigHash {
                            config_type: AttackCruiserConfigType::Global,
                            config_name: "global config value".to_string(),
                        },
                    },
                    definition: AttackCruiserConfigDefinition::Global(Box::new(
                        AttackCruiserGlobalConfig {
                            physics_speed: 1.0,
                            connect_timeout_seconds: 10.0,
                            ready_timeout_seconds: 10.0,
                            default_timeout_seconds: 120.0,
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
                            screen_relative_turning: AttackCruiserBool(true),
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
                            player_death_spawn_config: AttackCruiserConfigReference {
                                config_type: AttackCruiserConfigType::DeathSpawn,
                                config_name: "player death spawn config value".to_string(),
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
                    name: AttackCruiserConfigName {
                        hash: AttackCruiserConfigHash {
                            config_type: AttackCruiserConfigType::Game,
                            config_name: "game config value".to_string(),
                        },
                    },
                    definition: AttackCruiserConfigDefinition::Game(Box::new(
                        AttackCruiserGameConfig {
                            id: 27001,
                            encounter_id: 0,
                            sound_id: 2413,
                            mode: 1,
                            global_config: AttackCruiserConfigReference {
                                config_type: AttackCruiserConfigType::Global,
                                config_name: "global config value".to_string(),
                            },
                            end_condition_config: AttackCruiserConfigReference {
                                config_type: AttackCruiserConfigType::Condition,
                                config_name: "".to_string(),
                            },
                            win_condition_config: AttackCruiserConfigReference {
                                config_type: AttackCruiserConfigType::Condition,
                                config_name: "".to_string(),
                            },
                            target_value1: 999,
                            target_value2: 888,
                            playfield_height: 940.559,
                            playfield_length: 1500.0,
                            playfield_width: 1500.0,
                            playfield_warning_length: 1500.0,
                            playfield_warning_width: 1500.0,
                            playfield_center_x: 3.99,
                            playfield_center_z: -1993.09,
                            kill_zone_height: 0.0,
                            enemy_attack_radius: 50.0,
                            endless_waves: AttackCruiserBool(false),
                            debugged_actors: 0,
                            global_tilt_init_x: 0.0,
                            global_tilt_init_z: 0.0,
                            global_tilt_rate_x: 0.0,
                            global_tilt_rate_z: 0.0,
                            planet: AttackCruiserPlanetConfig {
                                model_id: 583,
                                pos: Pos3 {
                                    x: 0.0,
                                    y: 1363.94,
                                    z: 28539.1,
                                },
                                rotation_speed: 0.01,
                            },
                            players: AttackCruiserVec(
                                "players".to_string(),
                                vec![AttackCruiserPlayerConfig {
                                    ship_config: AttackCruiserConfigReference {
                                        config_type: AttackCruiserConfigType::Ship,
                                        config_name: "ship config value".to_string(),
                                    },
                                    camera_config: AttackCruiserConfigReference {
                                        config_type: AttackCruiserConfigType::Camera,
                                        config_name: "camera config value".to_string(),
                                    },
                                    lives: 5,
                                    spawn_pos: Pos3 {
                                        x: 0.0,
                                        y: 0.0,
                                        z: 0.0,
                                    },
                                    spawn_heading: 0.0,
                                }],
                            ),
                            events: AttackCruiserVec(
                                "events".to_string(),
                                vec![AttackCruiserEventConfig {
                                    event_type: 1,
                                    cinematics: AttackCruiserVec(
                                        "event cinematics".to_string(),
                                        vec![
                                            // AttackCruiserEventCinematicConfig {
                                            //     total_seconds: 15.0,
                                            //     animation_id: 10319,
                                            //     camera_heading: 0.0,
                                            //     camera_fov: 50.0,
                                            //     flip_camera_z: AttackCruiserBool(false),
                                            //     pre_wipe_style: 2,
                                            //     post_wipe_style: 2,
                                            // },
                                            AttackCruiserEventCinematicConfig {
                                                total_seconds: 15.0,
                                                animation_id: 10317,
                                                camera_heading: 0.0,
                                                camera_fov: 50.0,
                                                flip_camera_z: AttackCruiserBool(false),
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                            },
                                        ],
                                    ),
                                    event_actors: AttackCruiserVec::new(),
                                }],
                            ),
                            actor_pools: AttackCruiserVec(
                                "actor pools".to_string(),
                                vec![AttackCruiserActorPoolConfig {
                                    actor_config: AttackCruiserConfigReference {
                                        config_type: AttackCruiserConfigType::Ship,
                                        config_name: "ship config value".to_string(),
                                    },
                                    size: 500,
                                }],
                            ),
                            waves: AttackCruiserVec(
                                "waves".to_string(),
                                vec![AttackCruiserGameWaveConfig {
                                    wave_config: AttackCruiserConfigReference {
                                        config_type: AttackCruiserConfigType::Wave,
                                        config_name: "wave config value".to_string(),
                                    },
                                    launch_condition_config: AttackCruiserConfigReference {
                                        config_type: AttackCruiserConfigType::Condition,
                                        config_name: "niceshot".to_string(),
                                    },
                                    complete_condition_config: AttackCruiserConfigReference {
                                        config_type: AttackCruiserConfigType::Condition,
                                        config_name: "".to_string(),
                                    },
                                    remove_actors_on_completion: AttackCruiserBool(false),
                                }],
                            ),
                        },
                    )),
                },
                config3: AttackCruiserConfig {
                    name: AttackCruiserConfigName {
                        hash: AttackCruiserConfigHash {
                            config_type: AttackCruiserConfigType::Camera,
                            config_name: "main camera config value".to_string(),
                        },
                    },
                    definition: AttackCruiserConfigDefinition::Camera(Box::new(
                        AttackCruiserCameraConfig {
                            distance: 1000.0,
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
                configs: vec![
                    AttackCruiserConfig {
                        name: AttackCruiserConfigName {
                            hash: AttackCruiserConfigHash {
                                config_type: AttackCruiserConfigType::ComplexPhysics,
                                config_name: "physics config value".to_string(),
                            },
                        },
                        definition: AttackCruiserConfigDefinition::ComplexPhysics(Box::new(
                            AttackCruiserComplexPhysicsConfig {
                                base_config: AttackCruiserBasePhysicsConfig {
                                    contact_response: AttackCruiserBool(true),
                                    mass: 100.0,
                                    length: 35.11076,
                                    width: 24.00168,
                                    height: 5.4446693,
                                    center_of_mass_z: 0.0,
                                    max_speed: 100.0,
                                    vertical_speed: 0.0,
                                },
                                reverse_speed: -100.0,
                                turbo_speed: 200.0,
                                stationary_turn: 1.0,
                                gears: AttackCruiserVec(
                                    "physics config gears".to_string(),
                                    vec![AttackCruiserComplexPhysicsGear {
                                        shift_up_speed: 100.0,
                                        shift_down_speed: 100.0,
                                        base_acceleration: 20.0,
                                        base_deceleration: 20.0,
                                        turbo_acceleration: 50.0,
                                        brake_deceleration: 50.0,
                                        sideways_deceleration: 20.0,
                                        angular_acceleration: 1.0,
                                        turbo_angular_acceleration: 1.0,
                                        angular_deceleration: 1.0,
                                        max_angular_speed: 1.0,
                                        turbo_max_angular_speed: 1.0,
                                    }],
                                ),
                            },
                        )),
                    },
                    AttackCruiserConfig {
                        name: AttackCruiserConfigName {
                            hash: AttackCruiserConfigHash {
                                config_type: AttackCruiserConfigType::Ship,
                                config_name: "ship config value".to_string(),
                            },
                        },
                        definition: AttackCruiserConfigDefinition::Ship(Box::new(
                            AttackCruiserShipConfig {
                                actor_config: AttackCruiserActorConfig {
                                    model_id: 167,
                                    effect_id: 0,
                                    death_effect_id: 0,
                                    despawn_effect_id: 0,
                                    explode_offset: 1.0,
                                    collision_asset_name: "Ship_RepublicFrigate_bbe.cdt"
                                        .to_string(),
                                    physics_config: AttackCruiserConfigReference {
                                        config_type: AttackCruiserConfigType::ComplexPhysics,
                                        config_name: "physics config value".to_string(),
                                    },
                                    max_health: 100,
                                    explosive_collision: AttackCruiserBool(true),
                                    collision_damage: 0,
                                    score: 123,
                                    bonus_score: 0,
                                    bonus_max_age_seconds: 10.0,
                                    overhead_offset_y: 0.0,
                                    overhead_health_scale: 1.0,
                                    animations: AttackCruiserVec::new(),
                                    cinematics: AttackCruiserVec::new(),
                                    damage_states: AttackCruiserVec(
                                        "damage states".to_string(),
                                        vec![
                                            AttackCruiserActorDamageStateConfig {
                                                min_health_percent: 100.0,
                                                texture_alias: "damage0".to_string(),
                                                effects: AttackCruiserVec::new(),
                                            },
                                            AttackCruiserActorDamageStateConfig {
                                                min_health_percent: 80.0,
                                                texture_alias: "damage1".to_string(),
                                                effects: AttackCruiserVec::new(),
                                            },
                                            AttackCruiserActorDamageStateConfig {
                                                min_health_percent: 60.0,
                                                texture_alias: "damage2".to_string(),
                                                effects: AttackCruiserVec::new(),
                                            },
                                            AttackCruiserActorDamageStateConfig {
                                                min_health_percent: 20.0,
                                                texture_alias: "damage3".to_string(),
                                                effects: AttackCruiserVec::new(),
                                            },
                                        ],
                                    ),
                                },
                                thruster_effect_id: 1707,
                                invulnerable_effect_id: 1744,
                                stun_effect_id: 102,
                                weapons: AttackCruiserVec(
                                    "player weapons".to_string(),
                                    vec![AttackCruiserShipWeaponConfig {
                                        weapon_bay_config: AttackCruiserConfigReference {
                                            config_type: AttackCruiserConfigType::Blaster,
                                            config_name: "player weapon bay config value"
                                                .to_string(),
                                        },
                                        group: 1,
                                        tier: 1,
                                        special_weapon: AttackCruiserBool(false),
                                        exit_offset_x: 1.0,
                                        exit_offset_y: 1.0,
                                        exit_offset_z: 1.0,
                                        exit_offset_angle: 1.0,
                                        exit_min_angle: 1.0,
                                        exit_max_angle: 360.0,
                                    }],
                                ),
                                roll_max_angle: 30.0,
                                pitch_max_angle: 0.0,
                                continuous_fire_seconds: 10.0,
                                fire_cooldown_seconds: 0.0,
                            },
                        )),
                    },
                    AttackCruiserConfig {
                        name: AttackCruiserConfigName {
                            hash: AttackCruiserConfigHash {
                                config_type: AttackCruiserConfigType::Blaster,
                                config_name: "player weapon bay config value".to_string(),
                            },
                        },
                        definition: AttackCruiserConfigDefinition::Blaster(Box::new(
                            AttackCruiserBlasterConfig {
                                weapon_bay_config: AttackCruiserWeaponBayConfig {
                                    exit_velocity: 100.0,
                                    life_time_seconds: 100.0,
                                    reload_time_seconds: 0.0,
                                },
                                blaster_effect_id: 1558,
                                impact_effect_id: 0,
                                collision_damage: 25,
                                width: 100.0,
                                length: 100.0,
                                auto_fire: AttackCruiserBool(false),
                                auto_fire_range: 0.0,
                                penetrate: AttackCruiserBool(false),
                            },
                        )),
                    },
                    AttackCruiserConfig {
                        name: AttackCruiserConfigName {
                            hash: AttackCruiserConfigHash {
                                config_type: AttackCruiserConfigType::Camera,
                                config_name: "camera config value".to_string(),
                            },
                        },
                        definition: AttackCruiserConfigDefinition::Camera(Box::new(
                            AttackCruiserCameraConfig {
                                distance: 100.0,
                                min_distance: 0.0,
                                max_distance: 10000.0,
                                pitch: 10.0,
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
                    AttackCruiserConfig {
                        name: AttackCruiserConfigName {
                            hash: AttackCruiserConfigHash {
                                config_type: AttackCruiserConfigType::Wave,
                                config_name: "wave config value config value".to_string(),
                            },
                        },
                        definition: AttackCruiserConfigDefinition::Wave(Box::new(
                            AttackCruiserWaveConfig {
                                actors: AttackCruiserVec::new(),
                                hud_messages: AttackCruiserVec::new(),
                            },
                        )),
                    },
                    AttackCruiserConfig {
                        name: AttackCruiserConfigName {
                            hash: AttackCruiserConfigHash {
                                config_type: AttackCruiserConfigType::Actor,
                                config_name: "player death spawn actor config value".to_string(),
                            },
                        },
                        definition: AttackCruiserConfigDefinition::Actor(Box::new(
                            AttackCruiserActorConfig {
                                model_id: 167,
                                effect_id: 0,
                                death_effect_id: 0,
                                despawn_effect_id: 0,
                                explode_offset: 1.0,
                                collision_asset_name: "Ship_RepublicFrigate_bbe.cdt".to_string(),
                                physics_config: AttackCruiserConfigReference {
                                    config_type: AttackCruiserConfigType::ComplexPhysics,
                                    config_name: "physics config value".to_string(),
                                },
                                max_health: 100,
                                explosive_collision: AttackCruiserBool(true),
                                collision_damage: 0,
                                score: 123,
                                bonus_score: 0,
                                bonus_max_age_seconds: 10.0,
                                overhead_offset_y: 0.0,
                                overhead_health_scale: 1.0,
                                animations: AttackCruiserVec::new(),
                                cinematics: AttackCruiserVec::new(),
                                damage_states: AttackCruiserVec(
                                    "damage states".to_string(),
                                    vec![
                                        AttackCruiserActorDamageStateConfig {
                                            min_health_percent: 100.0,
                                            texture_alias: "damage0".to_string(),
                                            effects: AttackCruiserVec::new(),
                                        },
                                        AttackCruiserActorDamageStateConfig {
                                            min_health_percent: 80.0,
                                            texture_alias: "damage1".to_string(),
                                            effects: AttackCruiserVec::new(),
                                        },
                                        AttackCruiserActorDamageStateConfig {
                                            min_health_percent: 60.0,
                                            texture_alias: "damage2".to_string(),
                                            effects: AttackCruiserVec::new(),
                                        },
                                        AttackCruiserActorDamageStateConfig {
                                            min_health_percent: 20.0,
                                            texture_alias: "damage3".to_string(),
                                            effects: AttackCruiserVec::new(),
                                        },
                                    ],
                                ),
                            },
                        )),
                    },
                    AttackCruiserConfig {
                        name: AttackCruiserConfigName {
                            hash: AttackCruiserConfigHash {
                                config_type: AttackCruiserConfigType::DeathSpawn,
                                config_name: "player death spawn config value".to_string(),
                            },
                        },
                        definition: AttackCruiserConfigDefinition::DeathSpawn(Box::new(
                            AttackCruiserDeathSpawnConfig {
                                enable_chance: 100.0,
                                spawn_config: AttackCruiserVec(
                                    "death spawns".to_string(),
                                    vec![AttackCruiserSpawnConfig {
                                        actor_config: AttackCruiserConfigReference {
                                            config_type: AttackCruiserConfigType::Actor,
                                            config_name: "player death spawn actor config value"
                                                .to_string(),
                                        },
                                        ai_config: AttackCruiserConfigReference {
                                            config_type: AttackCruiserConfigType::Ai,
                                            config_name: "niceshot".to_string(),
                                        },
                                        chance: 100.0,
                                        forward_velocity: 1.0,
                                        count: 3,
                                        lifespan: 100.0,
                                    }],
                                ),
                            },
                        )),
                    },
                ],
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
                unknown2: 100,
                actor_pool_id: 0x4DB6C82AEC9B4E53,
                pos: Pos3 {
                    x: 3.99,
                    y: 1000.0,
                    z: -1993.09,
                },
                speed: Pos3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                heading: 0.0,
                unknown7: 100,
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
                state: AttackCruiserPlayerState {
                    unknown1: Some(AttackCruiserPlayerStateUnknown1 {
                        player_index: 1,
                        actor_id: 500,
                        unknown_value4: 200,
                        unknown4: "test".to_string(),
                        unknown5: "hello world".to_string(),
                    }),
                    score: None,
                    unknown3: None,
                    special_weapon: None,
                    unknown5: None,
                },
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserConfigPlayer {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::ConfigPlayer as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                guid: 1,
                config1: None,
                config2: None,
                config3: Some(AttackCruiserConfig {
                    name: AttackCruiserConfigName {
                        hash: AttackCruiserConfigHash {
                            config_type: AttackCruiserConfigType::Camera,
                            config_name: "player camera config value".to_string(),
                        },
                    },
                    definition: AttackCruiserConfigDefinition::Camera(Box::new(
                        AttackCruiserCameraConfig {
                            distance: 100.0,
                            min_distance: 0.0,
                            max_distance: 10000.0,
                            pitch: 0.0,
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
                }),
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
                game_state: 3,
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

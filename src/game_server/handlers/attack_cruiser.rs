use std::{
    io::{Cursor, Read},
    time::Instant,
};

use packet_serialize::DeserializePacket;
use serde::Deserialize;

use crate::game_server::{
    handlers::{
        character::{MinigameMatchmakingGroup, MinigameStatus},
        minigame::{
            handle_minigame_packet_write, MinigameRemovePlayerResult, SharedMinigameTypeData,
        },
        unique_guid::player_guid,
    },
    packets::{
        attack_cruiser::{
            AttackCruiserActorConfig, AttackCruiserActorDamageStateConfig,
            AttackCruiserActorPoolConfig, AttackCruiserAddActor, AttackCruiserAddPlayer,
            AttackCruiserBasePhysicsConfig, AttackCruiserBool, AttackCruiserCameraConfig,
            AttackCruiserChallengeMode, AttackCruiserClientConfig, AttackCruiserClientState,
            AttackCruiserComplexPhysicsConfig, AttackCruiserComplexPhysicsGear,
            AttackCruiserEventCinematicConfig, AttackCruiserEventConfig, AttackCruiserGameConfig,
            AttackCruiserGlobalConfig, AttackCruiserHostility, AttackCruiserHudMessageConfig,
            AttackCruiserOpCode, AttackCruiserPlanetConfig, AttackCruiserPlayerStateIndex,
            AttackCruiserPlayerStateInventory, AttackCruiserPlayerStateScore,
            AttackCruiserPlayerStateType, AttackCruiserPlayerStateUnknown3,
            AttackCruiserPlayerStateUnknown5, AttackCruiserPlayerStateUpdate,
            AttackCruiserPlayerUpdate, AttackCruiserRequestUpdatePlayers, AttackCruiserShipConfig,
            AttackCruiserStartupConfig, AttackCruiserStartupConfigClass,
            AttackCruiserStartupConfigDefinition, AttackCruiserStartupConfigHash,
            AttackCruiserStartupConfigReference, AttackCruiserUpdateClientState,
            AttackCruiserUpdatePlayers, AttackCruiserVec,
        },
        minigame::MinigameHeader,
        tunnel::TunneledPacket,
        ui::ExecuteScriptWithStringParams,
        GamePacket, Pos3,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

const SCORE_MULTIPLIER_TIERS: [u16; 5] = [100, 200, 300, 400, 500];

#[derive(Clone, Debug)]
struct AttackCruiserPlayerState {
    pub ready: bool,
    pub score: i32,
    pub score_multiplier_tier_progress: u16,
    pub score_multiplier_tier: u8,
    pub lives: u8,
    pub health: u16,
}

impl AttackCruiserPlayerState {
    pub fn new(lives: u8, health: u16, ready: bool) -> Self {
        AttackCruiserPlayerState {
            ready,
            score: 0,
            score_multiplier_tier_progress: 0,
            score_multiplier_tier: 1,
            lives,
            health,
        }
    }
}

#[derive(Clone, Debug)]
enum AttackCruiserGameState {
    WaitingForPlayersReady,
    WaveActive,
    GameOver,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserConfig {
    lives: u8,
    max_health: u16,
}

pub fn process_attack_cruiser_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let header = MinigameHeader::deserialize(cursor)?;
    handle_minigame_packet_write(
        sender,
        game_server,
        &header,
        |_, _, _, _, shared_minigame_data, _| {
            let SharedMinigameTypeData::AttackCruiser { game } = &mut shared_minigame_data.data
            else {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!(
                        "Received Attack Cruiser packet from unexpected game: {}, {buffer:x?}",
                        header.sub_op_code
                    ),
                ));
            };

            match AttackCruiserOpCode::try_from(header.sub_op_code) {
                Ok(op_code) => match op_code {
                    AttackCruiserOpCode::RequestUpdatePlayers => {
                        let request = AttackCruiserRequestUpdatePlayers::deserialize(cursor)?;
                        game.update_client_players(sender, request.update_type)
                    }
                    AttackCruiserOpCode::RoundTrip => Ok(Vec::new()),
                    _ => {
                        let mut buffer = Vec::new();
                        cursor.read_to_end(&mut buffer)?;
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::UnknownOpCode,
                            format!(
                                "Unimplemented Attack Cruiser op code: {op_code:?} {buffer:x?}"
                            ),
                        ))
                    }
                },
                Err(_) => {
                    let mut buffer = Vec::new();
                    cursor.read_to_end(&mut buffer)?;
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::UnknownOpCode,
                        format!(
                            "Unknown Attack Cruiser packet: {}, {buffer:x?}",
                            header.sub_op_code
                        ),
                    ))
                }
            }
        },
    )
}

#[derive(Clone, Debug)]
pub struct AttackCruiserGame {
    config: AttackCruiserConfig,
    player1: u32,
    player2: Option<u32>,
    player_states: [AttackCruiserPlayerState; 2],
    state: AttackCruiserGameState,
    players: Vec<u32>,
    group: MinigameMatchmakingGroup,
}

impl AttackCruiserGame {
    pub fn new(
        config: AttackCruiserConfig,
        player1: u32,
        player2: Option<u32>,
        group: MinigameMatchmakingGroup,
    ) -> Self {
        let mut players = vec![player1];
        if let Some(player2) = player2 {
            players.push(player2);
        }
        AttackCruiserGame {
            player1,
            player2,
            player_states: [
                AttackCruiserPlayerState::new(config.lives, config.max_health, false),
                AttackCruiserPlayerState::new(config.lives, config.max_health, player2.is_none()),
            ],
            state: AttackCruiserGameState::WaitingForPlayersReady,
            players,
            group,
            config,
        }
    }

    pub fn start(&self, sender: u32) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;

        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserClientConfig {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::ClientConfig as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                global_config: AttackCruiserStartupConfig::new(
                    "global config value".to_string(),
                    AttackCruiserStartupConfigDefinition::Global(Box::new(
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
                            player_perfect_aim_radius: 0.0,
                            player_auto_aim_assistance: 0.0,
                            npc_auto_aim_assistance: 0.0,
                            player_blaster_trapezoid_width: 1000.0,
                            player_auto_aim_range: 0.0,
                            npc_auto_aim_range: 0.0,
                            player_blaster_vertical_range: 100.0,
                            npc_blaster_vertical_range: 100.0,
                            min_blaster_speed: 0.0,
                            max_blaster_angle: 360.0,
                            projectile_ray_advance_seconds: 1.0,
                            projectile_ray_spacing: 1.0,
                            projectile_ray_iterations: 100,
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
                            wave_end_sound_id: 5400,
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
                            player_death_spawn_config: AttackCruiserStartupConfigReference {
                                class: AttackCruiserStartupConfigClass::DeathSpawn,
                                name: "".to_string(),
                            },
                            out_of_bounds_hud_message: AttackCruiserHudMessageConfig {
                                speaker_name_id: 0,
                                speaker_image_id: 0,
                                message_id: 0,
                                sound_id: 0,
                                duration_millis: 0,
                                delay_millis: 0,
                            },
                        },
                    )),
                ),
                game_config: AttackCruiserStartupConfig::new(
                    "game config value".to_string(),
                    AttackCruiserStartupConfigDefinition::Game(Box::new(AttackCruiserGameConfig {
                        id: 27001,
                        encounter_id: 0,
                        sound_id: 2413,
                        challenge_mode: AttackCruiserChallengeMode::Unlimited,
                        global_config: AttackCruiserStartupConfigReference {
                            class: AttackCruiserStartupConfigClass::Global,
                            name: "global config value".to_string(),
                        },
                        end_condition_config: AttackCruiserStartupConfigReference {
                            class: AttackCruiserStartupConfigClass::Condition,
                            name: "".to_string(),
                        },
                        win_condition_config: AttackCruiserStartupConfigReference {
                            class: AttackCruiserStartupConfigClass::Condition,
                            name: "".to_string(),
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
                        endless_waves: AttackCruiserBool(true),
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
                        players: AttackCruiserVec::new(),
                        events: AttackCruiserVec(
                            "events".to_string(),
                            vec![AttackCruiserEventConfig {
                                event_type: 1,
                                cinematics: AttackCruiserVec(
                                    "event cinematics".to_string(),
                                    vec![AttackCruiserEventCinematicConfig {
                                        total_seconds: 15.0,
                                        animation_id: 10317,
                                        camera_heading: 0.0,
                                        camera_fov: 50.0,
                                        flip_camera_z: AttackCruiserBool(false),
                                        pre_wipe_style: 2,
                                        post_wipe_style: 2,
                                    }],
                                ),
                                event_actors: AttackCruiserVec::new(),
                            }],
                        ),
                        actor_pools: AttackCruiserVec(
                            "actor pools".to_string(),
                            vec![AttackCruiserActorPoolConfig {
                                actor_config: AttackCruiserStartupConfigReference {
                                    class: AttackCruiserStartupConfigClass::Ship,
                                    name: "ship config value".to_string(),
                                },
                                size: 500,
                            }],
                        ),
                        waves: AttackCruiserVec::new(),
                    })),
                ),
                camera_config: AttackCruiserStartupConfig::new(
                    "main camera config value".to_string(),
                    AttackCruiserStartupConfigDefinition::Camera(Box::new(
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
                            particle_update_distance: 100000.0,
                            actor_update_radius: 100000.0,
                            shadow_quality: 20,
                            shadow_draw_distance: 30000.0,
                            shadow_blob_render_distance: 20000.0,
                            overhead_render_distance: 10000.0,
                        },
                    )),
                ),
                configs: vec![
                    AttackCruiserStartupConfig::new(
                        "physics config value".to_string(),
                        AttackCruiserStartupConfigDefinition::ComplexPhysics(Box::new(
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
                    ),
                    AttackCruiserStartupConfig::new(
                        "ship config value".to_string(),
                        AttackCruiserStartupConfigDefinition::Ship(Box::new(
                            AttackCruiserShipConfig {
                                actor_config: AttackCruiserActorConfig {
                                    model_id: 167,
                                    effect_id: 0,
                                    death_effect_id: 0,
                                    despawn_effect_id: 0,
                                    explode_offset: 1.0,
                                    collision_asset_name: "Ship_RepublicDestroyer_bbe.cdt"
                                        .to_string(),
                                    physics_config: AttackCruiserStartupConfigReference {
                                        class: AttackCruiserStartupConfigClass::ComplexPhysics,
                                        name: "physics config value".to_string(),
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
                                weapons: AttackCruiserVec::new(),
                                roll_max_angle: 30.0,
                                pitch_max_angle: 0.0,
                                continuous_fire_seconds: 10.0,
                                fire_cooldown_seconds: 1.0,
                            },
                        )),
                    ),
                    AttackCruiserStartupConfig::new(
                        "camera config value".to_string(),
                        AttackCruiserStartupConfigDefinition::Camera(Box::new(
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
                                particle_update_distance: 100000.0,
                                actor_update_radius: 100000.0,
                                shadow_quality: 20,
                                shadow_draw_distance: 30000.0,
                                shadow_blob_render_distance: 20000.0,
                                overhead_render_distance: 10000.0,
                            },
                        )),
                    ),
                ],
            },
        })];

        packets.append(
            &mut self
                .add_player_to_client(player_index, AttackCruiserPlayerStateType::default())?,
        );
        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserUpdateClientState {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::UpdateClientState as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                client_state: AttackCruiserClientState::Intro,
            },
        }));

        Ok(packets)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        Vec::new()
    }

    pub fn pause_or_resume(
        &mut self,
        player: u32,
        pause: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        Ok(Vec::new())
    }

    pub fn remove_player(
        &self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<MinigameRemovePlayerResult, ProcessPacketError> {
        Ok(MinigameRemovePlayerResult {
            broadcasts: Vec::new(),
            characters_to_remove: Vec::new(),
            end_game_for_all: false,
        })
    }

    pub fn update_client_players(
        &mut self,
        sender: u32,
        update_type: AttackCruiserPlayerStateType,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;

        let already_ready = self.player_states[player_index as usize].ready;
        let has_ready_update_type = update_type.score;

        let mut broadcasts = match (already_ready, has_ready_update_type) {
            (false, true) => {
                let mut broadcasts = vec![Broadcast::Single(
                    sender,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: ExecuteScriptWithStringParams {
                            script_name: "NotificationHandler.hideNotification".to_string(),
                            params: vec!["ACClickToStart".to_string()],
                        },
                    })],
                )];

                if !self.is_singleplayer() {
                    let other_player_index = (player_index + 1) % 2;
                    broadcasts.push(Broadcast::Single(
                        sender,
                        self.add_player_to_client(other_player_index, update_type)?,
                    ));
                }

                broadcasts.append(&mut self.update_client_players_once_ready(sender, update_type)?);
                self.player_states[player_index as usize].ready = true;

                Ok(broadcasts)
            }
            (true, _) => self.update_client_players_once_ready(sender, update_type),
            _ => Ok(Vec::new()),
        }?;

        if self
            .player_states
            .iter()
            .all(|player_state| player_state.ready)
            && matches!(self.state, AttackCruiserGameState::WaitingForPlayersReady)
        {
            broadcasts.append(&mut self.start_first_wave()?);
        }

        Ok(broadcasts)
    }

    fn add_player_to_client(
        &self,
        player_index: u8,
        update_type: AttackCruiserPlayerStateType,
    ) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let AttackCruiserGameState::WaitingForPlayersReady = &self.state else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Tried to add player index {player_index} to client, but the game isn't waiting for readiness ({self:?})")
            ));
        };

        Ok(vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserAddActor {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::AddActor as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    actor_id: self.player_actor_id(player_index),
                    hostility: AttackCruiserHostility::Friendly,
                    actor_config: AttackCruiserStartupConfigHash {
                        name: "ship config value".to_string(),
                        class: AttackCruiserStartupConfigClass::Ship,
                    },
                    pos: Pos3 {
                        x: 3.9,
                        y: 1000.0,
                        z: -1999.0,
                    },
                    speed: Pos3::default(),
                    heading: 0.0,
                    unknown7: 0,
                },
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserAddPlayer {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::AddPlayer as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    guid: player_guid(self.players[player_index as usize]),
                    state: self.player_state_update(player_index, update_type),
                },
            }),
        ])
    }

    fn update_client_players_once_ready(
        &self,
        sender: u32,
        update_type: AttackCruiserPlayerStateType,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        Ok(vec![Broadcast::Single(
            sender,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserUpdatePlayers {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::AddPlayer as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    states: self
                        .players
                        .iter()
                        .map(|guid| {
                            let player_index = self
                                .player_index(*guid)
                                .expect("GUID in players list is not actually a player");
                            AttackCruiserPlayerUpdate {
                                index: player_index.into(),
                                state: self.player_state_update(player_index, update_type),
                            }
                        })
                        .collect(),
                },
            })],
        )])
    }

    fn start_first_wave(&mut self) -> Result<Vec<Broadcast>, ProcessPacketError> {
        self.state = AttackCruiserGameState::WaveActive;
        Ok(vec![Broadcast::Multi(
            self.players.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserUpdateClientState {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::UpdateClientState as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    client_state: AttackCruiserClientState::WaveActive,
                },
            })],
        )])
    }

    fn is_singleplayer(&self) -> bool {
        self.player2.is_none()
    }

    fn player_index(&self, player_guid: u32) -> Result<u8, ProcessPacketError> {
        if player_guid == self.player1 {
            Ok(0)
        } else if Some(player_guid) == self.player2 {
            Ok(1)
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Player {player_guid} isn't one of the Attack Cruiser game's players ({self:?})")
            ))
        }
    }

    fn player_actor_id(&self, player_index: u8) -> i32 {
        (player_index + 1).into()
    }

    fn player_state_update(
        &self,
        player_index: u8,
        update_type: AttackCruiserPlayerStateType,
    ) -> AttackCruiserPlayerStateUpdate {
        let player_state = &self.player_states[player_index as usize];

        AttackCruiserPlayerStateUpdate {
            index: match update_type.index {
                true => Some(AttackCruiserPlayerStateIndex {
                    player_index: (player_index + 1).into(),
                    actor_id: self.player_actor_id(player_index),
                    unknown_value4: 0,
                    unknown4: "".to_string(),
                    unknown5: "".to_string(),
                }),
                false => None,
            },
            score: match update_type.score {
                true => Some(AttackCruiserPlayerStateScore {
                    score: player_state.score,
                    score_multiplier_tier_progress: player_state
                        .score_multiplier_tier_progress
                        .into(),
                    score_multiplier_tier_goal: SCORE_MULTIPLIER_TIERS
                        [player_state.score_multiplier_tier as usize]
                        .into(),
                    score_multiplier_tier: player_state.score_multiplier_tier.into(),
                    pain: 0,
                    lives: player_state.lives.into(),
                }),
                false => None,
            },
            unknown3: match update_type.unknown3 {
                true => Some(AttackCruiserPlayerStateUnknown3 {
                    actor_id: self.player_actor_id(player_index),
                    unknown_value4: 0,
                }),
                false => None,
            },
            inventory: match update_type.inventory {
                true => Some(AttackCruiserPlayerStateInventory {
                    // TODO: handle inventory
                    weapon_tier: 0,
                    primary_quantity: 0,
                    special_quantity: 0,
                    unknown4: 0,
                    special_icon_id: 0,
                    special_id: 0,
                }),
                false => None,
            },
            unknown5: match update_type.unknown5 {
                true => Some(AttackCruiserPlayerStateUnknown5 {
                    actor_id: self.player_actor_id(player_index),
                }),
                false => None,
            },
        }
    }
}

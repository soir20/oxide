use std::{
    cmp::Reverse,
    collections::BTreeMap,
    io::{Cursor, Read},
    time::{Duration, Instant},
};

use packet_serialize::DeserializePacket;
use priority_queue::PriorityQueue;
use rand::{thread_rng, Rng};
use serde::Deserialize;

use crate::game_server::{
    handlers::{
        character::{MinigameMatchmakingGroup, MinigameStatus},
        direction,
        minigame::{
            handle_minigame_packet_write, MinigameRemovePlayerResult, SharedMinigameTypeData,
        },
        unique_guid::player_guid,
    },
    packets::{
        attack_cruiser::{
            AttackCruiserActorConfig, AttackCruiserActorDamageStateConfig,
            AttackCruiserActorPoolConfig, AttackCruiserActorState, AttackCruiserActorUpdate,
            AttackCruiserAddActor, AttackCruiserAddPlayer, AttackCruiserAddProjectile,
            AttackCruiserBasePhysicsConfig, AttackCruiserBool, AttackCruiserBoolCommand,
            AttackCruiserCameraConfig, AttackCruiserChallengeMode, AttackCruiserClickedLocation,
            AttackCruiserClientConfig, AttackCruiserClientState, AttackCruiserCommand,
            AttackCruiserComplexPhysicsConfig, AttackCruiserComplexPhysicsGear,
            AttackCruiserEventCinematicConfig, AttackCruiserEventConfig, AttackCruiserGameConfig,
            AttackCruiserGlobalConfig, AttackCruiserHostility, AttackCruiserHudMessageConfig,
            AttackCruiserOpCode, AttackCruiserPlanetConfig, AttackCruiserPlayerStateIndex,
            AttackCruiserPlayerStateInventory, AttackCruiserPlayerStateScore,
            AttackCruiserPlayerStateType, AttackCruiserPlayerStateUnknown3,
            AttackCruiserPlayerStateUnknown5, AttackCruiserPlayerStateUpdate,
            AttackCruiserPlayerUpdate, AttackCruiserQueueCommand,
            AttackCruiserRequestUpdatePlayers, AttackCruiserShipStartupConfig,
            AttackCruiserStartupConfig, AttackCruiserStartupConfigClass,
            AttackCruiserStartupConfigDefinition, AttackCruiserStartupConfigHash,
            AttackCruiserStartupConfigReference, AttackCruiserUpdateClientActors,
            AttackCruiserUpdateClientState, AttackCruiserUpdatePlayers,
            AttackCruiserUpdateServerActors, AttackCruiserVec,
        },
        minigame::MinigameHeader,
        tunnel::TunneledPacket,
        ui::ExecuteScriptWithStringParams,
        GamePacket, Pos, Pos3,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

const SCORE_MULTIPLIER_TIERS: [u16; 5] = [100, 200, 300, 400, 500];

fn rotate(origin: Pos3, yaw: f32, pitch: f32) -> Pos3 {
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let x1 = origin.x * cos_yaw + origin.z * sin_yaw;
    let y1 = origin.y;
    let z1 = -origin.x * sin_yaw + origin.z * cos_yaw;

    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    let x2 = x1;
    let y2 = y1 * cos_pitch - z1 * sin_pitch;
    let z2 = y1 * sin_pitch + z1 * cos_pitch;

    Pos3 {
        x: x2,
        y: y2,
        z: z2,
    }
}

fn corners(
    origin: Pos3,
    length: f32,
    width: f32,
    height: f32,
    yaw: f32,
    pitch: f32,
) -> (Pos3, Pos3) {
    let half_size = Pos3 {
        x: length / 2.0,
        y: height / 2.0,
        z: width / 2.0,
    };

    let corner1 = rotate(origin - half_size, yaw, pitch);
    let corner2 = rotate(origin + half_size, yaw, pitch);

    (corner1, corner2)
}

#[derive(Clone, Debug)]
struct AttackCruiserPlayerState {
    pub ready: bool,
    pub pos: Pos3,
    pub heading: f32,
    pub speed: Pos3,
    pub angular_speed: f32,
    pub forward_multiplier: f32,
    pub turn_multiplier: f32,
    pub score: i32,
    pub score_multiplier_tier_progress: u16,
    pub score_multiplier_tier: u8,
    pub lives: u8,
    pub health: u16,
    pub primary_weapon_tier: usize,
}

impl AttackCruiserPlayerState {
    pub fn new(lives: u8, pos: Pos3, heading: f32, health: u16, ready: bool) -> Self {
        AttackCruiserPlayerState {
            ready,
            pos,
            heading,
            speed: Pos3::default(),
            angular_speed: 0.0,
            forward_multiplier: 0.0,
            turn_multiplier: 0.0,
            score: 0,
            score_multiplier_tier_progress: 0,
            score_multiplier_tier: 1,
            lives,
            health,
            primary_weapon_tier: 0,
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
struct AttackCruiserSpawnLocation {
    pos: Pos3,
    heading: f32,
}

const fn default_yaw_degrees() -> f32 {
    0.0
}

const fn default_wobble_degrees() -> f32 {
    3.0
}

const fn default_speed() -> f32 {
    500.0
}

const fn default_lifetime_millis() -> f32 {
    3.0
}

const fn default_count() -> u8 {
    1
}

const fn default_launch_offset() -> f32 {
    30.0
}

const fn default_launch_height() -> f32 {
    10.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserProjectile {
    pub composite_effect_id: u32,
    #[serde(default = "default_yaw_degrees")]
    pub yaw_degrees: f32,
    #[serde(default = "default_wobble_degrees")]
    pub wobble_degrees: f32,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_lifetime_millis")]
    pub lifetime_millis: f32,
    pub length: f32,
    pub width: f32,
    #[serde(default = "default_count")]
    pub count: u8,
    #[serde(default = "default_launch_offset")]
    pub launch_offset: f32,
    #[serde(default = "default_launch_height")]
    pub launch_height: f32,
    pub damage: i16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserPlayerPrimaryWeapon {
    pub projectiles: Vec<AttackCruiserProjectile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserPlayerWeaponConfig {
    pub cooldown_millis: f32,
    pub primary_tiers: Vec<AttackCruiserPlayerPrimaryWeapon>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserShipConfig {
    pub model_id: u32,
    pub length: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserConfig {
    lives: u8,
    max_health: u16,
    spawn1: AttackCruiserSpawnLocation,
    spawn2: AttackCruiserSpawnLocation,
    player_ship: AttackCruiserShipConfig,
    player_weapons: AttackCruiserPlayerWeaponConfig,
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
                    AttackCruiserOpCode::UpdateActors => {
                        let client_states = AttackCruiserUpdateClientActors::deserialize(cursor)?;
                        game.handle_client_actor_update(sender, client_states)
                    }
                    AttackCruiserOpCode::ClickedLocation => {
                        let click = AttackCruiserClickedLocation::deserialize(cursor)?;
                        game.handle_click(sender, click)
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
struct AttackCruiserProjectileInstance {
    launched_by_actor_id: i32,
    speed: Pos3,
    origin_corner1: Pos3,
    origin_corner2: Pos3,
    launch_time: Instant,
    damage: i16,
}

#[derive(Clone, Debug)]
struct AttackCruiserProjectileSpawn {
    pub projectile_id: i32,
    pub effect_id: u32,
    pub lifetime_millis: f32,
    pub origin: Pos3,
    pub speed: Pos3,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug)]
struct AttackCruiserProjectilePool {
    live_projectiles: BTreeMap<i32, AttackCruiserProjectileInstance>,
    expiry: PriorityQueue<i32, Reverse<Instant>>,
}

impl AttackCruiserProjectilePool {
    pub fn new() -> Self {
        AttackCruiserProjectilePool {
            live_projectiles: BTreeMap::new(),
            expiry: PriorityQueue::new(),
        }
    }

    pub fn launch(
        &mut self,
        launched_by_actor_id: i32,
        actor_origin: Pos3,
        direction: Pos3,
        projectile: &AttackCruiserProjectile,
    ) -> Result<Vec<AttackCruiserProjectileSpawn>, ProcessPacketError> {
        self.expire();

        let rng = &mut thread_rng();
        let mut launched_projectiles = Vec::new();

        for _ in 0..projectile.count {
            let projectile_id = self.next_id()?;

            let wobble = rng
                .gen_range(-projectile.wobble_degrees..=projectile.wobble_degrees)
                .to_radians();
            let relative_yaw = projectile.yaw_degrees.to_radians() + wobble;

            let launch_offset = direction
                + direction
                    * Pos3 {
                        x: projectile.launch_offset,
                        y: 0.0,
                        z: projectile.launch_offset,
                    }
                + Pos3 {
                    x: 0.0,
                    y: projectile.launch_height,
                    z: 0.0,
                };

            let origin = actor_origin + launch_offset;
            let speed = rotate(
                Pos3 {
                    x: direction.x,
                    y: 0.0,
                    z: direction.z,
                },
                relative_yaw,
                wobble,
            ) * projectile.speed;
            let yaw = direction.x.atan2(direction.z) + relative_yaw;
            let pitch = wobble;

            let (corner1, corner2) =
                corners(origin, projectile.length, projectile.width, 0.0, yaw, pitch);

            let now = Instant::now();
            let expiry_time = now
                .checked_add(Duration::from_secs_f32(projectile.lifetime_millis * 1000.0))
                .ok_or_else(|| {
                    ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Tried to launch a projectile, but {now:?} + {}ms would overflow",
                            projectile.lifetime_millis
                        ),
                    )
                })?;
            self.live_projectiles.insert(
                projectile_id,
                AttackCruiserProjectileInstance {
                    launched_by_actor_id,
                    speed,
                    origin_corner1: corner1,
                    origin_corner2: corner2,
                    launch_time: now,
                    damage: projectile.damage,
                },
            );
            self.expiry.push(projectile_id, Reverse(expiry_time));

            launched_projectiles.push(AttackCruiserProjectileSpawn {
                projectile_id,
                effect_id: projectile.composite_effect_id,
                lifetime_millis: projectile.lifetime_millis,
                origin,
                speed,
                yaw,
                pitch,
            });
        }

        Ok(launched_projectiles)
    }

    pub fn expire(&mut self) {
        let now = Instant::now();
        loop {
            let removable_projectile_id =
                self.expiry
                    .peek()
                    .and_then(|(projectile_id, expiry)| match expiry.0 <= now {
                        true => Some(*projectile_id),
                        false => None,
                    });

            match removable_projectile_id {
                Some(projectile_id) => {
                    self.expiry.remove(&projectile_id);
                    self.live_projectiles.remove(&projectile_id);
                }
                None => break,
            }
        }
    }

    fn next_id(&mut self) -> Result<i32, ProcessPacketError> {
        match self
            .live_projectiles
            .last_key_value()
            .and_then(|(id, _)| id.checked_add(1))
        {
            Some(next_id) => Ok(next_id),
            None => {
                (1..=i32::MAX).into_iter().find(|id| !self.live_projectiles.contains_key(id)).ok_or_else(|| ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    "Tried to launch a projectile, but the game is at the maximum number of projectiles".to_string()
                ))
            }
        }
    }
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
    projectiles: AttackCruiserProjectilePool,
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
                AttackCruiserPlayerState::new(
                    config.lives,
                    config.spawn1.pos,
                    config.spawn1.heading,
                    config.max_health,
                    false,
                ),
                AttackCruiserPlayerState::new(
                    config.lives,
                    config.spawn2.pos,
                    config.spawn2.heading,
                    config.max_health,
                    player2.is_none(),
                ),
            ],
            state: AttackCruiserGameState::WaitingForPlayersReady,
            players,
            group,
            config,
            projectiles: AttackCruiserProjectilePool::new(),
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
                            client_update_actors_interval_seconds: 0.1,
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
                            health_foreground_image_id: 163,
                            health_background_image_id: 164,
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
                            angular_speed: 0.01,
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
                                    length: self.config.player_ship.length,
                                    width: self.config.player_ship.width,
                                    height: self.config.player_ship.height,
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
                            AttackCruiserShipStartupConfig {
                                actor_config: AttackCruiserActorConfig {
                                    model_id: self.config.player_ship.model_id,
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
                                    overhead_health_scale: 0.5,
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
                                continuous_fire_seconds: 0.05,
                                fire_cooldown_seconds: self.config.player_weapons.cooldown_millis
                                    / 1000.0,
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

    pub fn handle_client_actor_update(
        &mut self,
        sender: u32,
        client_states: AttackCruiserUpdateClientActors,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;

        for client_state in client_states.states.into_iter() {
            if client_state.actor_id == self.player_actor_id(player_index) {
                let player_state = &mut self.player_states[player_index as usize];
                player_state.pos = client_state.pos;
                player_state.heading = client_state.heading;
                player_state.speed = client_state.speed;
                player_state.angular_speed = client_state.angular_speed;
                player_state.forward_multiplier = client_state.forward_multiplier;
                player_state.turn_multiplier = client_state.turn_multiplier;
            }
        }

        let player_state = &self.player_states[player_index as usize];
        Ok(vec![Broadcast::Multi(
            self.players.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserUpdateServerActors {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::UpdateActors as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    states: vec![AttackCruiserActorUpdate {
                        actor_id: self.player_actor_id(player_index),
                        pos: player_state.pos,
                        heading: player_state.heading,
                        speed: player_state.speed,
                        angular_speed: player_state.angular_speed,
                        forward_multiplier: player_state.forward_multiplier,
                        turn_multiplier: player_state.turn_multiplier,
                        health: player_state.health.into(),
                        state: AttackCruiserActorState::default(),
                    }],
                },
            })],
        )])
    }

    pub fn handle_click(
        &mut self,
        sender: u32,
        click: AttackCruiserClickedLocation,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;
        let player_state = &self.player_states[player_index as usize];

        let direction = Pos3::from(direction(
            Pos {
                x: player_state.pos.x,
                y: 0.0,
                z: player_state.pos.z,
                w: 1.0,
            },
            Pos {
                x: click.clicked_pos.x,
                y: 0.0,
                z: click.clicked_pos.y,
                w: 1.0,
            },
        ));

        let mut packets = Vec::new();

        if let Some(primary_weapon) = self
            .config
            .player_weapons
            .primary_tiers
            .get(player_state.primary_weapon_tier)
        {
            for projectile in primary_weapon.projectiles.iter() {
                packets.extend(
                    self.projectiles
                        .launch(
                            self.player_actor_id(player_index),
                            player_state.pos,
                            direction,
                            projectile,
                        )?
                        .into_iter()
                        .map(|launched_projectile| {
                            GamePacket::serialize(&TunneledPacket {
                                unknown1: true,
                                inner: AttackCruiserAddProjectile {
                                    minigame_header: MinigameHeader {
                                        stage_guid: self.group.stage_guid,
                                        sub_op_code: AttackCruiserOpCode::AddProjectile as i32,
                                        stage_group_guid: self.group.stage_group_guid,
                                    },
                                    projectile_id: launched_projectile.projectile_id,
                                    unknown2: 0,
                                    effect_id: projectile.composite_effect_id,
                                    despawn_effect_id: 0,
                                    lifetime_seconds: projectile.lifetime_millis * 1000.0,
                                    origin: launched_projectile.origin,
                                    speed: launched_projectile.speed,
                                    unknown8: Pos3::default(),
                                    yaw: launched_projectile.yaw,
                                    pitch: launched_projectile.pitch,
                                    unknown11: 0.0,
                                    unknown12: 0.0,
                                    unknown13: 0,
                                },
                            })
                        }),
                );
            }
        }

        Ok(vec![Broadcast::Multi(self.players.clone(), packets)])
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
                    pos: self.player_states[player_index as usize].pos,
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
        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserUpdateClientState {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::UpdateClientState as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                client_state: AttackCruiserClientState::WaveActive,
            },
        })];

        for (player_index, guid) in self.players.iter().enumerate() {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserQueueCommand {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::QueueCommand as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    actor_id: self.player_actor_id(player_index as u8),
                    command: AttackCruiserCommand::Movable(AttackCruiserBoolCommand {
                        guid: player_guid(*guid),
                        value: true,
                    }),
                },
            }));
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserQueueCommand {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::QueueCommand as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    actor_id: self.player_actor_id(player_index as u8),
                    command: AttackCruiserCommand::Collision(AttackCruiserBoolCommand {
                        guid: player_guid(*guid),
                        value: true,
                    }),
                },
            }));
        }

        Ok(vec![Broadcast::Multi(self.players.clone(), packets)])
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

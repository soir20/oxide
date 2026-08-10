use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    io::{Cursor, Read},
    sync::Arc,
    time::{Duration, Instant},
};

use glam::{EulerRot, Quat, Vec3};
use oxide_bvh::Bvh;
use packet_serialize::DeserializePacket;
use priority_queue::PriorityQueue;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use smallvec::SmallVec;

use crate::{
    game_server::{
        handlers::{
            character::{MinigameMatchmakingGroup, MinigameStatus},
            direction,
            minigame::{
                handle_minigame_packet_write, MinigameCountdown, MinigameRemovePlayerResult,
                SharedMinigameTypeData,
            },
            unique_guid::player_guid,
        },
        packets::{
            attack_cruiser::{
                AttackCruiserActorAnimationConfig, AttackCruiserActorAnimationType,
                AttackCruiserActorCinematicConfig, AttackCruiserActorCinematicType,
                AttackCruiserActorConfig, AttackCruiserActorDamageStateConfig,
                AttackCruiserActorPoolConfig, AttackCruiserActorState, AttackCruiserActorUpdate,
                AttackCruiserAddActor, AttackCruiserAddPlayer, AttackCruiserAddProjectile,
                AttackCruiserBasePhysicsConfig, AttackCruiserBool, AttackCruiserBoolCommand,
                AttackCruiserChallengeMode, AttackCruiserClickedLocation,
                AttackCruiserClientConfig, AttackCruiserClientState, AttackCruiserCommand,
                AttackCruiserComplexPhysicsConfig, AttackCruiserComplexPhysicsGear,
                AttackCruiserEventCinematicConfig, AttackCruiserEventConfig,
                AttackCruiserGameConfig, AttackCruiserGlobalConfig, AttackCruiserHostility,
                AttackCruiserHudMessageConfig, AttackCruiserOpCode, AttackCruiserPlanetConfig,
                AttackCruiserPlayerStateActorId, AttackCruiserPlayerStateIndex,
                AttackCruiserPlayerStateInventory, AttackCruiserPlayerStateScore,
                AttackCruiserPlayerStateType, AttackCruiserPlayerStateUnknown3,
                AttackCruiserPlayerStateUpdate, AttackCruiserPlayerUpdate,
                AttackCruiserQueueCommand, AttackCruiserRemoveActor, AttackCruiserRemovePlayer,
                AttackCruiserRemoveProjectile, AttackCruiserRequestUpdatePlayers,
                AttackCruiserShipStartupConfig, AttackCruiserStartupCameraConfig,
                AttackCruiserStartupConfig, AttackCruiserStartupConfigClass,
                AttackCruiserStartupConfigDefinition, AttackCruiserStartupConfigHash,
                AttackCruiserStartupConfigReference, AttackCruiserUpdateClientActors,
                AttackCruiserUpdateClientState, AttackCruiserUpdatePlayers,
                AttackCruiserUpdateServerActors, AttackCruiserVec,
            },
            minigame::MinigameHeader,
            player_update::HudMessage,
            tunnel::TunneledPacket,
            ui::ExecuteScriptWithStringParams,
            GamePacket, Pos, Pos3,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    info,
};

const SCORE_MULTIPLIER_TIERS: [u16; 5] = [100, 200, 300, 400, 500];

fn is_inside_oval(pos: Pos3, oval_center: Pos3, oval_radius_x: f32, oval_radius_z: f32) -> bool {
    let delta_x = pos.x - oval_center.x;
    let delta_z = pos.z - oval_center.z;

    let rx_sq = oval_radius_x * oval_radius_x;
    let rz_sq = oval_radius_z * oval_radius_z;

    (delta_x * delta_x) * rz_sq + (delta_z * delta_z) * rx_sq <= rx_sq * rz_sq
}

fn rotate(origin: Pos3, yaw: f32, pitch: f32) -> Pos3 {
    let rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    (rotation * Vec3::from(origin)).into()
}

fn show_hud_message(
    recipients: &[u32],
    message_id: u32,
    duration_millis: u32,
    name_id: Option<u32>,
    image_id: Option<u32>,
    sound_id: Option<u32>,
) -> Broadcast {
    Broadcast::Multi(
        recipients.to_vec(),
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: HudMessage {
                unknown1: 0,
                unknown2: 0,
                name_id: name_id.unwrap_or_default(),
                image_id: image_id.unwrap_or_default(),
                message_id,
                sound_id: sound_id.unwrap_or_default(),
                duration_millis,
                unknown5: 0,
            },
        })],
    )
}

#[derive(Clone, Debug)]
struct AttackCruiserActor {
    pub id: i32,
    pub pos: Pos3,
    pub yaw: f32,
    pub speed: Pos3,
    pub angular_speed: f32,
    pub forward_multiplier: f32,
    pub turn_multiplier: f32,
    pub health: u16,
    pub bvh: Option<Arc<Bvh>>,
    pub max_roll: f32,
}

impl AttackCruiserActor {
    pub fn dead(&self) -> bool {
        self.health == 0
    }
}

#[derive(Clone, Debug, Default)]
enum AttackCruiserPlayerBoundsState {
    #[default]
    Inside,
    Outside {
        timer: MinigameCountdown,
    },
    OutsideWaitingToWarp {
        timer: MinigameCountdown,
    },
}

impl AttackCruiserPlayerBoundsState {
    pub fn pause_or_resume(&mut self, pause: bool) {
        match self {
            AttackCruiserPlayerBoundsState::Outside { timer } => timer.pause_or_resume(pause),
            AttackCruiserPlayerBoundsState::OutsideWaitingToWarp { timer } => {
                timer.pause_or_resume(pause)
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
struct AttackCruiserPlayer {
    pub ready: bool,
    pub actor: AttackCruiserActor,
    pub score: i32,
    pub score_multiplier_tier_progress: u16,
    pub score_multiplier_tier: u8,
    pub lives: u8,
    pub primary_weapon_tier: usize,
    pub invulnerability_timer: MinigameCountdown,
    pub bounds_state: AttackCruiserPlayerBoundsState,
    pub bounds_warning_hud_timer: MinigameCountdown,
}

impl AttackCruiserPlayer {
    pub fn new(
        player_index: u8,
        lives: u8,
        pos: Pos3,
        yaw: f32,
        health: u16,
        ready: bool,
        bvh: Option<Arc<Bvh>>,
        max_roll: f32,
    ) -> Self {
        AttackCruiserPlayer {
            ready,
            actor: AttackCruiserActor {
                id: player_actor_id(player_index, lives),
                pos,
                yaw,
                speed: Pos3::default(),
                angular_speed: 0.0,
                forward_multiplier: 0.0,
                turn_multiplier: 0.0,
                health,
                bvh,
                max_roll,
            },
            score: 0,
            score_multiplier_tier_progress: 0,
            score_multiplier_tier: 1,
            lives,
            primary_weapon_tier: 0,
            invulnerability_timer: MinigameCountdown::new(),
            bounds_state: AttackCruiserPlayerBoundsState::default(),
            bounds_warning_hud_timer: MinigameCountdown::new(),
        }
    }

    pub fn respawnable(&self, now: Instant) -> bool {
        self.lives > 0 && self.completed_death(now)
    }

    pub fn lost(&self, now: Instant) -> bool {
        self.lives == 0 && self.completed_death(now)
    }

    pub fn respawn(&mut self, health: u16, invulnerability_duration: Duration, now: Instant) {
        self.actor.health = health;
        self.invulnerability_timer
            .schedule_event(invulnerability_duration, now);
    }

    pub fn dead(&self) -> bool {
        self.actor.dead() || self.lives == 0
    }

    pub fn trackable(&self) -> bool {
        !self.actor.dead()
            && !matches!(
                self.bounds_state,
                AttackCruiserPlayerBoundsState::Outside { .. }
            )
    }

    pub fn vulnerable(&self, now: Instant) -> bool {
        self.trackable()
            && self
                .invulnerability_timer
                .time_until_next_event(now)
                .is_zero()
    }

    pub fn damage(&mut self, damage: i16, now: Instant, respawn_millis: u32) {
        self.actor.health = self.actor.health.saturating_sub_signed(damage);

        if self.actor.dead() {
            self.lives = self.lives.saturating_sub(1);
            self.invulnerability_timer
                .schedule_event(Duration::from_millis(respawn_millis.into()), now);
        }
    }

    pub fn paused(&self) -> bool {
        self.invulnerability_timer.paused()
    }

    pub fn disabled(&self) -> bool {
        self.dead() || self.paused()
    }

    pub fn disarmed(&self) -> bool {
        self.disabled()
            || matches!(
                self.bounds_state,
                AttackCruiserPlayerBoundsState::Outside { .. }
            )
    }

    fn completed_death(&self, now: Instant) -> bool {
        self.actor.dead()
            && self
                .invulnerability_timer
                .time_until_next_event(now)
                .is_zero()
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
    yaw_degrees: f32,
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
    4.0
}

const fn default_screen_relative_turning() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserCameraConfig {
    default_distance: f32,
    min_distance: f32,
    max_distance: f32,
    pitch_degrees: f32,
    #[serde(default)]
    offset_z: f32,
    target_tracking_high_level_quotient: f32,
    zoom_step_quantization: f32,
    zoom_step_high_level_quotient: f32,
    near_clip_distance: f32,
    #[serde(default = "default_screen_relative_turning")]
    screen_relative_turning: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserHealthBarConfig {
    foreground_image_id: u32,
    background_image_id: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserProjectile {
    pub composite_effect_id: u32,
    pub hit_composite_effect_id: u32,
    #[serde(default = "default_yaw_degrees")]
    pub yaw_degrees: f32,
    #[serde(default = "default_wobble_degrees")]
    pub wobble_degrees: f32,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_lifetime_millis")]
    pub lifetime_millis: f32,
    #[serde(default = "default_count")]
    pub count: u8,
    #[serde(default = "default_launch_offset")]
    pub launch_offset: f32,
    #[serde(default = "default_launch_height")]
    pub launch_height: f32,
    pub length: f32,
    pub damage: i16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlayerPrimaryWeapon {
    pub projectiles: Vec<Arc<AttackCruiserProjectile>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlayerWeaponConfig {
    pub cooldown_millis: f32,
    pub primary_tiers: Vec<AttackCruiserPlayerPrimaryWeapon>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserShipConfig {
    pub model_id: u32,
    pub asset_name: String,
    pub max_roll_degrees: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlayerConfig {
    pub lives: u8,
    pub max_health: u16,
    pub respawn_millis: u32,
    pub post_respawn_invulnerability_millis: u32,
    pub out_of_bounds_warp_millis: u32,
    pub out_of_bounds_warp_delay_millis: u32,
    pub spawn1: AttackCruiserSpawnLocation,
    pub spawn2: AttackCruiserSpawnLocation,
    pub ship: AttackCruiserShipConfig,
    pub weapons: AttackCruiserPlayerWeaponConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlayfieldConfig {
    pub center: Pos3,
    pub radius_x: f32,
    pub radius_z: f32,
    pub warning_radius_ratio: f32,
    pub warning_message_id: u32,
    pub warning_millis: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserConfig {
    camera: AttackCruiserCameraConfig,
    health_bar: AttackCruiserHealthBarConfig,
    player: AttackCruiserPlayerConfig,
    playfield: AttackCruiserPlayfieldConfig,
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
    origin: Pos3,
    launch_time: Instant,
    projectile: Arc<AttackCruiserProjectile>,
}

const STEP_CACHE_STACK_LEN: usize = 5;
struct AttackCruiserProjectileStepCache {
    inv_rots: SmallVec<[Quat; STEP_CACHE_STACK_LEN]>,
    origins: SmallVec<[Vec3; STEP_CACHE_STACK_LEN]>,
    step_secs_end: SmallVec<[f32; STEP_CACHE_STACK_LEN]>,
}

#[derive(Clone, Debug)]
struct AttackCruiserProjectileSpawn {
    pub projectile_id: i32,
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
        projectile: &Arc<AttackCruiserProjectile>,
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
                    origin,
                    launch_time: now,
                    projectile: projectile.clone(),
                },
            );
            self.expiry.push(projectile_id, Reverse(expiry_time));

            launched_projectiles.push(AttackCruiserProjectileSpawn {
                projectile_id,
                origin,
                speed,
                yaw,
                pitch,
            });
        }

        Ok(launched_projectiles)
    }

    pub fn hits(
        &mut self,
        actor_id: i32,
        actor: &AttackCruiserActor,
        now: Instant,
        delta: Duration,
    ) -> Vec<(i32, Arc<AttackCruiserProjectile>)> {
        let Some(ship_bvh) = &actor.bvh else {
            return Vec::new();
        };

        let delta_secs = delta.as_secs_f32();
        let ship_pos = Vec3::from(actor.pos);
        let ship_roll = actor.max_roll * actor.turn_multiplier;
        let ship_velocity = actor.speed * actor.forward_multiplier;
        let ship_angular_velocity = actor.angular_speed * actor.turn_multiplier;

        let aabb = ship_bvh.aabb();
        let min_array: [f32; 3] = aabb.min.coords.into();
        let max_array: [f32; 3] = aabb.max.coords.into();

        let min_corner = Vec3::from(min_array);
        let max_corner = Vec3::from(max_array);
        let ship_radius = min_corner.distance(max_corner) * 0.5;

        let mut step_cache = BTreeMap::new();

        // There will almost always be fewer than 32 hits. 32 * 4 bytes = 128 bytes,
        // which fills two 64-byte L1 cache lines on most CPUs
        let projectile_ids: SmallVec<[i32; 32]> = self
            .live_projectiles
            .iter()
            .filter(|(_, projectile)| {
                if projectile.launched_by_actor_id == actor_id {
                    return false;
                }

                let projectile_speed = projectile.projectile.speed;
                let projectile_len = projectile.projectile.length;

                let secs_since_launch = now
                    .saturating_duration_since(projectile.launch_time)
                    .as_secs_f32();

                let global_start = Vec3::from(projectile.origin)
                    + Vec3::from(projectile.speed) * secs_since_launch;

                let max_projectile_travel = projectile_speed * delta_secs;
                let max_ship_travel = Vec3::from(ship_velocity).length() * delta_secs;
                let max_reach = max_projectile_travel + max_ship_travel + ship_radius;

                let dist_to_ship_sq = global_start.distance_squared(ship_pos);
                if dist_to_ship_sq > max_reach * max_reach {
                    return false;
                }

                let max_steps = ((projectile_speed * delta_secs / projectile_len)
                    .abs()
                    .ceil() as u32)
                    .max(1);

                let cache = step_cache.entry(max_steps).or_insert_with(|| {
                    let mut inv_rots = SmallVec::with_capacity(max_steps as usize);
                    let mut origins = SmallVec::with_capacity(max_steps as usize);
                    let mut cached_step_secs_end = SmallVec::with_capacity(max_steps as usize);

                    let step_secs = delta_secs / (max_steps as f32);
                    (1..=max_steps).for_each(|step| {
                        let step_secs_end = step_secs * step as f32;

                        let ship_origin = actor.pos + ship_velocity * step_secs_end;
                        let ship_yaw = actor.yaw + ship_angular_velocity * step_secs_end;

                        cached_step_secs_end.push(step_secs_end);
                        origins.push(Vec3::from(ship_origin));
                        inv_rots.push(
                            Quat::from_euler(EulerRot::YXZ, ship_yaw, 0.0, ship_roll).inverse(),
                        );
                    });

                    AttackCruiserProjectileStepCache {
                        inv_rots,
                        origins,
                        step_secs_end: cached_step_secs_end,
                    }
                });

                let global_speed = Vec3::from(projectile.speed);

                (0..(max_steps as usize)).any(|step_index| {
                    let step_start_secs = if step_index == 0 {
                        0.0
                    } else {
                        cache.step_secs_end[step_index - 1]
                    };
                    let step_secs_end = cache.step_secs_end[step_index];

                    let ship_origin_end = cache.origins[step_index];
                    let inv_rotation_end = cache.inv_rots[step_index];

                    let local_start = inv_rotation_end
                        * (global_start + global_speed * step_start_secs - ship_origin_end);
                    let local_end = inv_rotation_end
                        * (global_start + global_speed * step_secs_end - ship_origin_end);

                    let segment_vector = local_end - local_start;
                    let projectile_direction = segment_vector.normalize_or_zero();
                    let half_length_offset = projectile_direction * (projectile_len * 0.5);

                    let check_start = local_start - half_length_offset;
                    let check_end = local_end + half_length_offset;

                    !ship_bvh.has_line_of_sight(check_start.to_array(), check_end.to_array())
                })
            })
            .map(|(projectile_id, _)| *projectile_id)
            .collect();

        projectile_ids
            .into_iter()
            .map(|projectile_id| {
                (
                    projectile_id,
                    self.remove_unchecked(projectile_id).projectile,
                )
            })
            .collect()
    }

    pub fn expire(&mut self) {
        let now = Instant::now();
        while let Some((&projectile_id, Reverse(expiry))) = self.expiry.peek() {
            if expiry > &now {
                break;
            }
            self.expiry.pop();
            self.live_projectiles.remove(&projectile_id);
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

    fn remove_unchecked(&mut self, projectile_id: i32) -> AttackCruiserProjectileInstance {
        self.expiry.remove(&projectile_id);
        self.live_projectiles
            .remove(&projectile_id)
            .expect("Projectile should exist")
    }
}

fn player_actor_id(player_index: u8, lives: u8) -> i32 {
    (player_index * 2 + 1 + lives.is_multiple_of(2) as u8).into()
}

#[derive(Clone, Debug)]
pub struct AttackCruiserGame {
    config: Arc<AttackCruiserConfig>,
    player1: u32,
    player2: Option<u32>,
    player_states: [AttackCruiserPlayer; 2],
    state: AttackCruiserGameState,
    active_players: Vec<u32>,
    players: Vec<u32>,
    group: MinigameMatchmakingGroup,
    projectiles: AttackCruiserProjectilePool,
}

impl AttackCruiserGame {
    pub fn new(
        config: Arc<AttackCruiserConfig>,
        player1: u32,
        player2: Option<u32>,
        group: MinigameMatchmakingGroup,
        bvhs: &HashMap<String, Arc<Bvh>>,
    ) -> Self {
        let mut players = vec![player1];
        if let Some(player2) = player2 {
            players.push(player2);
        }

        let player_bvh = bvhs.get(&config.player.ship.asset_name).cloned();
        if player_bvh.is_none() {
            info!(
                "Missing BVH for Attack Cruiser player ship {}. Defaulting to empty BVH.",
                config.player.ship.asset_name
            );
        }

        AttackCruiserGame {
            player1,
            player2,
            player_states: [
                AttackCruiserPlayer::new(
                    0,
                    config.player.lives,
                    config.player.spawn1.pos,
                    config.player.spawn1.yaw_degrees.to_radians(),
                    config.player.max_health,
                    false,
                    player_bvh.clone(),
                    config.player.ship.max_roll_degrees.to_radians(),
                ),
                AttackCruiserPlayer::new(
                    1,
                    config.player.lives,
                    config.player.spawn2.pos,
                    config.player.spawn2.yaw_degrees.to_radians(),
                    config.player.max_health,
                    player2.is_none(),
                    player_bvh,
                    config.player.ship.max_roll_degrees.to_radians(),
                ),
            ],
            state: AttackCruiserGameState::WaitingForPlayersReady,
            active_players: players.clone(),
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
                            connect_timeout_seconds: 0.0,
                            ready_timeout_seconds: 0.0,
                            default_timeout_seconds: 0.0,
                            effects_preload_timeout_seconds: 0.0,
                            effects_ready_timeout_seconds: 0.0,
                            server_update_players_interval_seconds: 0.0,
                            server_update_actors_interval_seconds: 0.0,
                            server_draw_debug_data_interval_seconds: 0.0,
                            client_update_actors_interval_seconds: 0.0,
                            max_interpolation_step: 0.0,
                            small_mass_threshold: 0.0,
                            dodge_prediction_time: 0.0,
                            dodge_separation: 0.0,
                            player_perfect_aim_radius: 0.0,
                            player_auto_aim_assistance: 0.0,
                            npc_auto_aim_assistance: 0.0,
                            player_blaster_trapezoid_width: 0.0,
                            player_auto_aim_range: 0.0,
                            npc_auto_aim_range: 0.0,
                            player_blaster_vertical_range: 0.0,
                            npc_blaster_vertical_range: 0.0,
                            min_blaster_speed: 0.0,
                            max_blaster_angle: 0.0,
                            projectile_ray_advance_seconds: 0.0,
                            projectile_ray_spacing: 0.0,
                            projectile_ray_iterations: 0,
                            advance_launch_seconds: 0.0,
                            advance_interception_time: 0.0,
                            collisionless_time: 0,
                            tractionless_time: 0,
                            screen_relative_turning: AttackCruiserBool(
                                self.config.camera.screen_relative_turning,
                            ),
                            ship_to_ship_collision: AttackCruiserBool(false),
                            player_death_animation_delay_seconds: 0.0,
                            respawn_damage_area: 0.0,
                            respawn_delay_seconds: 0.0,
                            respawn_invulnerable_seconds: 0.0,
                            enable_composite_effects: AttackCruiserBool(true),
                            torpedo_reticule_effect_id: 0,
                            torpedo_reticule_effect_seconds: 0.0,
                            fighter_reticule_effect_id: 0,
                            fighter_reticule_effect_seconds: 0.0,
                            wave_end_sound_id: 0,
                            damage_warning_sound_id: 0,
                            damage_warning_interval_seconds: 0.0,
                            mine_deploy_sound_id: 0,
                            fighter_launch_sound_id: 0,
                            score_meter_tier1: 0,
                            score_decay_tier1: 0,
                            score_meter_exponent: 0.0,
                            score_decay_exponent: 0.0,
                            health_foreground_image_id: self.config.health_bar.foreground_image_id,
                            health_background_image_id: self.config.health_bar.background_image_id,
                            health_foreground_internal_id: 1,
                            health_background_internal_id: 2,
                            enable_weapon_tiers: AttackCruiserBool(false),
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
                        id: self.group.stage_guid,
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
                        playfield_height: self.config.playfield.center.y,
                        playfield_length: self.config.playfield.radius_x * 2.0,
                        playfield_width: self.config.playfield.radius_z * 2.0,
                        playfield_warning_length: 0.0,
                        playfield_warning_width: 0.0,
                        playfield_center_x: self.config.playfield.center.x,
                        playfield_center_z: self.config.playfield.center.z,
                        kill_zone_height: 0.0,
                        enemy_attack_radius: 0.0,
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
                        AttackCruiserStartupCameraConfig {
                            default_distance: self.config.camera.default_distance,
                            min_distance: self.config.camera.min_distance,
                            max_distance: self.config.camera.max_distance,
                            pitch: self.config.camera.pitch_degrees,
                            min_pitch: self.config.camera.pitch_degrees,
                            max_pitch: self.config.camera.pitch_degrees,
                            offset_z: self.config.camera.offset_z,
                            target_tracking_high_level_quotient: self
                                .config
                                .camera
                                .target_tracking_high_level_quotient,
                            zoom_step_quantization: self.config.camera.zoom_step_quantization,
                            zoom_step_high_level_quotient: self
                                .config
                                .camera
                                .zoom_step_high_level_quotient,
                            forward_tether: AttackCruiserBool(true),
                            forward_tether_seconds: 1.0,
                            near_clip_distance: self.config.camera.near_clip_distance,
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
                                    length: 1.0,
                                    width: 1.0,
                                    height: 1.0,
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
                                    model_id: self.config.player.ship.model_id,
                                    effect_id: 0,
                                    death_effect_id: 0,
                                    despawn_effect_id: 0,
                                    explode_offset: 0.0,
                                    collision_asset_name: format!(
                                        "{}.cdt",
                                        self.config.player.ship.asset_name
                                    ),
                                    physics_config: AttackCruiserStartupConfigReference {
                                        class: AttackCruiserStartupConfigClass::ComplexPhysics,
                                        name: "physics config value".to_string(),
                                    },
                                    max_health: self.config.player.max_health.into(),
                                    explosive_collision: AttackCruiserBool(false),
                                    collision_damage: 0,
                                    score: 0,
                                    bonus_score: 0,
                                    bonus_max_age_seconds: 0.0,
                                    overhead_offset_y: 0.0,
                                    overhead_health_scale: 0.5,
                                    animations: AttackCruiserVec(
                                        "animations".to_string(),
                                        vec![
                                            AttackCruiserActorAnimationConfig {
                                                animation_type:
                                                    AttackCruiserActorAnimationType::Death1,
                                                slot_id: 3001,
                                                loops: AttackCruiserBool(false),
                                                play_time_seconds: 6.0,
                                            },
                                            AttackCruiserActorAnimationConfig {
                                                animation_type:
                                                    AttackCruiserActorAnimationType::Death2,
                                                slot_id: 3002,
                                                loops: AttackCruiserBool(false),
                                                play_time_seconds: 6.0,
                                            },
                                            AttackCruiserActorAnimationConfig {
                                                animation_type:
                                                    AttackCruiserActorAnimationType::WarpIn,
                                                slot_id: 3010,
                                                loops: AttackCruiserBool(false),
                                                play_time_seconds: 3.0,
                                            },
                                            AttackCruiserActorAnimationConfig {
                                                animation_type:
                                                    AttackCruiserActorAnimationType::WarpOut,
                                                slot_id: 3009,
                                                loops: AttackCruiserBool(false),
                                                play_time_seconds: 2.0,
                                            },
                                        ],
                                    ),
                                    cinematics: AttackCruiserVec(
                                        "cinematics".to_string(),
                                        vec![
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death1,
                                                play_time_seconds: 6.0,
                                                animation_id: 10207,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death1,
                                                play_time_seconds: 6.0,
                                                animation_id: 10208,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death1,
                                                play_time_seconds: 6.0,
                                                animation_id: 10209,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death1,
                                                play_time_seconds: 6.0,
                                                animation_id: 10210,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death1,
                                                play_time_seconds: 6.0,
                                                animation_id: 10211,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death2,
                                                play_time_seconds: 6.0,
                                                animation_id: 10308,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death2,
                                                play_time_seconds: 6.0,
                                                animation_id: 10309,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death2,
                                                play_time_seconds: 6.0,
                                                animation_id: 10310,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death2,
                                                play_time_seconds: 6.0,
                                                animation_id: 10311,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Death2,
                                                play_time_seconds: 6.0,
                                                animation_id: 10312,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Warp,
                                                play_time_seconds: 3.0,
                                                animation_id: 10010,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                            AttackCruiserActorCinematicConfig {
                                                cinematic_type:
                                                    AttackCruiserActorCinematicType::Global,
                                                play_time_seconds: 2.0,
                                                animation_id: 10019,
                                                pre_wipe_style: 2,
                                                post_wipe_style: 2,
                                                post_camera_ease_in_seconds: 0.0,
                                            },
                                        ],
                                    ),
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
                                roll_max_angle: self.config.player.ship.max_roll_degrees,
                                pitch_max_angle: 0.0,
                                continuous_fire_seconds: 0.05,
                                fire_cooldown_seconds: self.config.player.weapons.cooldown_millis
                                    / 1000.0,
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

    pub fn tick(&mut self, now: Instant, tick_duration: Duration) -> Vec<Broadcast> {
        let mut broadcasts = Vec::new();
        let mut hits = Vec::new();

        for player_index in 0..self.players.len() {
            let player_state = &mut self.player_states[player_index];
            let in_bounds = is_inside_oval(
                player_state.actor.pos,
                self.config.playfield.center,
                self.config.playfield.radius_x,
                self.config.playfield.radius_z,
            );

            let mut update_clients = match (&mut player_state.bounds_state, in_bounds) {
                (AttackCruiserPlayerBoundsState::Inside, true) => false,
                (AttackCruiserPlayerBoundsState::Inside, false) => {
                    self.player_states[player_index].bounds_state =
                        AttackCruiserPlayerBoundsState::OutsideWaitingToWarp {
                            timer: MinigameCountdown::new_with_event(Duration::from_millis(
                                self.config.player.out_of_bounds_warp_delay_millis.into(),
                            )),
                        };
                    true
                }
                (AttackCruiserPlayerBoundsState::Outside { timer }, _) => {
                    if timer.time_until_next_event(now).is_zero() {
                        self.player_states[player_index].bounds_state =
                            AttackCruiserPlayerBoundsState::OutsideWaitingToWarp {
                                timer: MinigameCountdown::new_with_event(Duration::from_millis(
                                    self.config.player.out_of_bounds_warp_delay_millis.into(),
                                )),
                            };
                    }
                    true
                }
                (AttackCruiserPlayerBoundsState::OutsideWaitingToWarp { .. }, true) => {
                    self.player_states[player_index].bounds_state =
                        AttackCruiserPlayerBoundsState::Inside;
                    true
                }
                (AttackCruiserPlayerBoundsState::OutsideWaitingToWarp { timer }, false) => {
                    if timer.time_until_next_event(now).is_zero() {
                        self.player_states[player_index].bounds_state =
                            AttackCruiserPlayerBoundsState::Outside {
                                timer: MinigameCountdown::new_with_event(Duration::from_millis(
                                    self.config.player.out_of_bounds_warp_millis.into(),
                                )),
                            };
                    }
                    true
                }
            };

            let player_state = &mut self.player_states[player_index];
            let actor_id = player_state.actor.id;
            if player_state.respawnable(now) {
                player_state.respawn(
                    self.config.player.max_health,
                    Duration::from_millis(
                        self.config
                            .player
                            .post_respawn_invulnerability_millis
                            .into(),
                    ),
                    now,
                );

                let mut actor_packets = self.replace_client_player_actor(player_index as u8);
                actor_packets.append(&mut self.update_client_players_once_ready(
                    AttackCruiserPlayerStateType {
                        index: false,
                        score: false,
                        unknown3: false,
                        inventory: false,
                        actor_id: true,
                    },
                ));
                actor_packets.append(&mut self.set_player_frozen(player_index, false));
                broadcasts.push(Broadcast::Multi(self.active_players.clone(), actor_packets));
            } else if player_state.lost(now) {
                broadcasts.push(Broadcast::Single(
                    self.players[player_index],
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: ExecuteScriptWithStringParams {
                            script_name: "StarDestroyerHandler.quitGame".to_string(),
                            params: vec![],
                        },
                    })],
                ));
            }

            let player_state = &mut self.player_states[player_index];
            if player_state.trackable() {
                let actor = &mut player_state.actor;
                let mut actor_hits = self.projectiles.hits(actor_id, actor, now, tick_duration);
                let total_damage = Self::total_damage(&actor_hits);

                // If the player still has invulnerability time, process the hits but deal no damage
                if player_state.vulnerable(now) {
                    player_state.damage(total_damage, now, self.config.player.respawn_millis);

                    if player_state.dead() {
                        let mut death_packets = self.set_player_frozen(player_index, true);
                        death_packets.append(&mut self.update_client_players_once_ready(
                            AttackCruiserPlayerStateType {
                                index: false,
                                score: true,
                                unknown3: false,
                                inventory: false,
                                actor_id: false,
                            },
                        ));
                        broadcasts
                            .push(Broadcast::Multi(self.active_players.clone(), death_packets));

                        update_clients = true;
                    }
                }

                hits.append(&mut actor_hits);
            }

            if update_clients {
                broadcasts.push(self.update_server_actor(player_index as u8));
            }
        }

        broadcasts.push(Broadcast::Multi(
            self.active_players.clone(),
            hits.into_iter()
                .map(|(projectile_id, projectile)| {
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: AttackCruiserRemoveProjectile {
                            minigame_header: MinigameHeader {
                                stage_guid: self.group.stage_guid,
                                sub_op_code: AttackCruiserOpCode::RemoveProjectile as i32,
                                stage_group_guid: self.group.stage_group_guid,
                            },
                            projectile_id,
                            despawn_effect_id: projectile.hit_composite_effect_id,
                            delay_seconds: 0.0,
                        },
                    })
                })
                .collect(),
        ));

        broadcasts
    }

    pub fn pause_or_resume(
        &mut self,
        player: u32,
        pause: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        self.player_index(player)?;

        if !self.is_singleplayer() {
            return Ok(Vec::new());
        }

        self.player_states.iter_mut().for_each(|player_state| {
            player_state.invulnerability_timer.pause_or_resume(pause);
            player_state.bounds_state.pause_or_resume(pause);
            player_state.bounds_warning_hud_timer.pause_or_resume(pause);
        });
        Ok(Vec::new())
    }

    pub fn remove_player(
        &mut self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<MinigameRemovePlayerResult, ProcessPacketError> {
        let player_index = self.player_index(player)? as usize;

        let mut packets = self.despawn_client_player_actor(&self.player_states[player_index]);
        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserRemovePlayer {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::RemovePlayer as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                guid: player_guid(player),
            },
        }));
        let broadcasts = vec![Broadcast::Multi(self.active_players.clone(), packets)];

        self.active_players
            .retain(|active_player| *active_player != player);
        self.player_states[player_index].lives = 0;

        minigame_status.total_score = self.player_states[player_index].score;
        Ok(MinigameRemovePlayerResult {
            broadcasts,
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

                broadcasts.push(Broadcast::Single(
                    sender,
                    self.update_client_players_once_ready(update_type),
                ));
                self.player_states[player_index as usize].ready = true;

                broadcasts
            }
            (true, _) => vec![Broadcast::Single(
                sender,
                self.update_client_players_once_ready(update_type),
            )],
            _ => Vec::new(),
        };

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
        let player_state = &mut self.player_states[player_index as usize];

        if player_state.disabled() {
            return Ok(Vec::new());
        }

        let mut broadcasts = Vec::new();
        let now = Instant::now();

        for client_state in client_states.states.into_iter() {
            if client_state.actor_id == player_state.actor.id {
                player_state.actor.pos = client_state.pos;
                player_state.actor.yaw = client_state.yaw;
                player_state.actor.speed = client_state.speed;
                player_state.actor.angular_speed = client_state.angular_speed;
                player_state.actor.forward_multiplier = client_state.forward_multiplier;
                player_state.actor.turn_multiplier = client_state.turn_multiplier;

                let almost_outside_bounds = !is_inside_oval(
                    player_state.actor.pos,
                    self.config.playfield.center,
                    self.config.playfield.radius_x * self.config.playfield.warning_radius_ratio,
                    self.config.playfield.radius_z * self.config.playfield.warning_radius_ratio,
                );

                if almost_outside_bounds
                    && player_state
                        .bounds_warning_hud_timer
                        .time_until_next_event(now)
                        .is_zero()
                {
                    player_state.bounds_warning_hud_timer.schedule_event(
                        Duration::from_millis(self.config.playfield.warning_millis.into()),
                        now,
                    );
                    broadcasts.push(show_hud_message(
                        &[sender],
                        self.config.playfield.warning_message_id,
                        self.config.playfield.warning_millis,
                        None,
                        None,
                        None,
                    ));
                }
            }
        }

        broadcasts.push(self.update_server_actor(player_index));

        Ok(broadcasts)
    }

    pub fn handle_click(
        &mut self,
        sender: u32,
        click: AttackCruiserClickedLocation,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;
        let player_state = &self.player_states[player_index as usize];

        if player_state.disarmed() {
            return Ok(Vec::new());
        }

        let direction = Pos3::from(direction(
            Pos {
                x: player_state.actor.pos.x,
                y: 0.0,
                z: player_state.actor.pos.z,
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
            .player
            .weapons
            .primary_tiers
            .get(player_state.primary_weapon_tier)
        {
            for projectile in primary_weapon.projectiles.iter() {
                packets.extend(
                    self.projectiles
                        .launch(
                            player_state.actor.id,
                            player_state.actor.pos,
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

        Ok(vec![Broadcast::Multi(self.active_players.clone(), packets)])
    }

    fn spawn_client_player_actor(&self, player_state: &AttackCruiserPlayer) -> Vec<Vec<u8>> {
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserAddActor {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::AddActor as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                actor_id: player_state.actor.id,
                hostility: AttackCruiserHostility::Friendly,
                actor_config: AttackCruiserStartupConfigHash {
                    name: "ship config value".to_string(),
                    class: AttackCruiserStartupConfigClass::Ship,
                },
                pos: player_state.actor.pos,
                speed: player_state.actor.speed,
                yaw: player_state.actor.yaw,
                unknown7: 0,
            },
        })]
    }

    fn despawn_client_player_actor(&self, player_state: &AttackCruiserPlayer) -> Vec<Vec<u8>> {
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserRemoveActor {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::RemoveActor as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                actor_id: player_state.actor.id,
            },
        })]
    }

    fn replace_client_player_actor(&mut self, player_index: u8) -> Vec<Vec<u8>> {
        let mut packets =
            self.despawn_client_player_actor(&self.player_states[player_index as usize]);
        let player_state = &mut self.player_states[player_index as usize];
        player_state.actor.id = player_actor_id(player_index, player_state.lives);
        packets.append(
            &mut self.spawn_client_player_actor(&self.player_states[player_index as usize]),
        );
        packets
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

        let player_state = &self.player_states[player_index as usize];
        let mut packets = self.spawn_client_player_actor(player_state);
        packets.push(GamePacket::serialize(&TunneledPacket {
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
        }));

        Ok(packets)
    }

    fn update_client_players_once_ready(
        &self,
        update_type: AttackCruiserPlayerStateType,
    ) -> Vec<Vec<u8>> {
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserUpdatePlayers {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::UpdatePlayers as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                states: (0..self.players.len() as u8)
                    .map(|player_index| AttackCruiserPlayerUpdate {
                        player_index: player_index.into(),
                        state: self.player_state_update(player_index, update_type),
                    })
                    .collect(),
            },
        })]
    }

    fn update_server_actor(&self, player_index: u8) -> Broadcast {
        let player_state = &self.player_states[player_index as usize];
        let warp_out = !player_state.dead()
            && matches!(
                &player_state.bounds_state,
                AttackCruiserPlayerBoundsState::Outside { .. }
            );

        Broadcast::Multi(
            self.active_players.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserUpdateServerActors {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::UpdateActors as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    states: vec![AttackCruiserActorUpdate {
                        actor_id: player_state.actor.id,
                        pos: player_state.actor.pos,
                        yaw: player_state.actor.yaw,
                        speed: player_state.actor.speed,
                        angular_speed: player_state.actor.angular_speed,
                        forward_multiplier: player_state.actor.forward_multiplier,
                        turn_multiplier: player_state.actor.turn_multiplier,
                        health: player_state.actor.health.into(),
                        state: AttackCruiserActorState {
                            unknown1: false,
                            unknown2: false,
                            invulnerable: false,
                            unknown4: false,
                            unknown5: false,
                            unknown6: false,
                            unknown7: false,
                            dead_unused: false,
                            warp_in: false,
                            global_cinematic: false,
                            warp_out_animation: warp_out,
                            warp_end_game: false,
                            reset_speed_damage_state: warp_out,
                            unknown14: false,
                            unknown15: false,
                            hide_ring: warp_out,
                            dead: player_state.dead(),
                        },
                    }],
                },
            })],
        )
    }

    fn set_player_frozen(&self, player_index: usize, frozen: bool) -> Vec<Vec<u8>> {
        let state = &self.player_states[player_index];
        let guid = self.players[player_index];
        vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserQueueCommand {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::QueueCommand as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    actor_id: state.actor.id,
                    command: AttackCruiserCommand::Movable(AttackCruiserBoolCommand {
                        guid: player_guid(guid),
                        value: !frozen,
                    }),
                },
            }),
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserQueueCommand {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::QueueCommand as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    actor_id: state.actor.id,
                    command: AttackCruiserCommand::Collision(AttackCruiserBoolCommand {
                        guid: player_guid(guid),
                        value: !frozen,
                    }),
                },
            }),
        ]
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

        for player_index in 0..self.players.len() {
            packets.append(&mut self.set_player_frozen(player_index, false));
        }

        Ok(vec![Broadcast::Multi(self.active_players.clone(), packets)])
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

    fn player_state_update(
        &self,
        player_index: u8,
        update_type: AttackCruiserPlayerStateType,
    ) -> AttackCruiserPlayerStateUpdate {
        let player_state = &self.player_states[player_index as usize];
        AttackCruiserPlayerStateUpdate {
            index: match update_type.index {
                true => Some(AttackCruiserPlayerStateIndex {
                    player_index: player_index.into(),
                    actor_id: player_state.actor.id,
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
                    actor_id: player_state.actor.id,
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
            actor_id: match update_type.actor_id {
                true => Some(AttackCruiserPlayerStateActorId {
                    actor_id: player_state.actor.id,
                }),
                false => None,
            },
        }
    }

    fn total_damage(projectiles: &[(i32, Arc<AttackCruiserProjectile>)]) -> i16 {
        projectiles.iter().fold(0, |total_damage, (_, projectile)| {
            total_damage.saturating_add(projectile.damage)
        })
    }
}

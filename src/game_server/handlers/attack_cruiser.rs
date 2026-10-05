use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, HashMap, HashSet},
    f32::consts::PI,
    io::{Cursor, Read},
    iter,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use arrayvec::ArrayVec;
use glam::{EulerRot, Quat, Vec3};
use oxide_bvh::Bvh;
use packet_serialize::DeserializePacket;
use priority_queue::PriorityQueue;
use rand::{rngs::ThreadRng, thread_rng, Rng};
use serde::Deserialize;
use smallvec::SmallVec;

use crate::{
    config::Angle,
    debug,
    game_server::{
        handlers::{
            character::{MinigameMatchmakingGroup, MinigameStatus},
            direction, distance3_pos, distance3_sq,
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
                AttackCruiserActorDamageStateEffectConfig, AttackCruiserActorPoolConfig,
                AttackCruiserActorState, AttackCruiserActorUpdate, AttackCruiserAddActor,
                AttackCruiserAddPlayer, AttackCruiserAddProjectile, AttackCruiserBasePhysicsConfig,
                AttackCruiserBool, AttackCruiserBoolCommand, AttackCruiserChallengeMode,
                AttackCruiserCinematicStyle, AttackCruiserClickType, AttackCruiserClickedLocation,
                AttackCruiserClientConfig, AttackCruiserClientState, AttackCruiserCommand,
                AttackCruiserComplexPhysicsConfig, AttackCruiserComplexPhysicsGear,
                AttackCruiserCompositeEffect, AttackCruiserEventActorConfig,
                AttackCruiserEventCinematicConfig, AttackCruiserEventConfig,
                AttackCruiserEventType, AttackCruiserGameConfig, AttackCruiserGlobalConfig,
                AttackCruiserHostility, AttackCruiserHudMessageConfig, AttackCruiserOpCode,
                AttackCruiserPlanetStartupConfig, AttackCruiserPlayerStateActorId,
                AttackCruiserPlayerStateIndex, AttackCruiserPlayerStateInventory,
                AttackCruiserPlayerStateScore, AttackCruiserPlayerStateType,
                AttackCruiserPlayerStateUnknown3, AttackCruiserPlayerStateUpdate,
                AttackCruiserPlayerUpdate, AttackCruiserQueueCommand, AttackCruiserRemoveActor,
                AttackCruiserRemovePlayer, AttackCruiserRemoveProjectile,
                AttackCruiserRequestUpdatePlayers, AttackCruiserShipStartupConfig,
                AttackCruiserStartupCameraConfig, AttackCruiserStartupConfig,
                AttackCruiserStartupConfigClass, AttackCruiserStartupConfigDefinition,
                AttackCruiserStartupConfigHash, AttackCruiserStartupConfigReference,
                AttackCruiserUpdateClientActors, AttackCruiserUpdateClientState,
                AttackCruiserUpdatePlayers, AttackCruiserUpdateServerActors, AttackCruiserVec,
            },
            command::PlaySoundIdOnTarget,
            minigame::MinigameHeader,
            player_update::HudMessage,
            tunnel::TunneledPacket,
            ui::ExecuteScriptWithStringParams,
            GamePacket, Pos, Pos3, Target,
        },
        Broadcast, GameServer, LogLevel, ProcessPacketError, ProcessPacketErrorType,
    },
    info,
};

const SCORE_MULTIPLIER_TIERS: [u16; 5] = [100, 200, 300, 400, 500];
const TIME_EPSILON: f32 = 1e-5;

fn zero_nan(value: f32) -> f32 {
    match value.is_nan() {
        true => 0.0,
        false => value,
    }
}

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

fn gcd_u16(mut a: u16, mut b: u16) -> u16 {
    while b != 0 {
        let prev_divisor = b;
        b = a % b;
        a = prev_divisor;
    }
    a
}

struct AttackCruiserWeaponLaunchVector {
    origin: Pos3,
    direction: Pos3,
    speed: Pos3,
    yaw: f32,
    pitch: f32,
}

fn launch_vector(
    rng: &mut ThreadRng,
    actor_origin: Pos3,
    direction: Pos3,
    speed: f32,
    wobble: Angle,
    yaw: Angle,
    launch_offset: f32,
    launch_height: f32,
) -> AttackCruiserWeaponLaunchVector {
    let wobble = rng.gen_range(-wobble.to_radians()..=wobble.to_radians());
    let relative_yaw = yaw.to_radians() + wobble;

    let launch_offset = direction
        + direction
            * Pos3 {
                x: launch_offset,
                y: 0.0,
                z: launch_offset,
            }
        + Pos3 {
            x: 0.0,
            y: launch_height,
            z: 0.0,
        };

    let origin = actor_origin + launch_offset;
    let wobbled_direction = rotate(direction, relative_yaw, wobble);
    let speed = wobbled_direction * speed;

    let yaw = wobbled_direction.x.atan2(wobbled_direction.z);
    let pitch = -zero_nan(wobbled_direction.y).asin();

    AttackCruiserWeaponLaunchVector {
        origin,
        direction: wobbled_direction,
        speed,
        yaw,
        pitch,
    }
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

fn normalize_angle(angle_radians: f32) -> f32 {
    (angle_radians + PI).rem_euclid(2.0 * PI) - PI
}

fn normalize_angle_positive(angle_radians: f32) -> f32 {
    angle_radians.rem_euclid(2.0 * PI)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AttackCruiserActorInvulnerabilityPhase {
    Dead,
    Respawning,
    UsedPowerup,
    #[default]
    Vulnerable,
}

#[derive(Clone, Debug)]
struct AttackCruiserActorInvulnerability {
    phase: AttackCruiserActorInvulnerabilityPhase,
    timer: MinigameCountdown,
}

impl Default for AttackCruiserActorInvulnerability {
    fn default() -> Self {
        AttackCruiserActorInvulnerability {
            phase: AttackCruiserActorInvulnerabilityPhase::default(),
            timer: MinigameCountdown::new(),
        }
    }
}

impl AttackCruiserActorInvulnerability {
    pub fn has_phase(&self, phase: AttackCruiserActorInvulnerabilityPhase) -> bool {
        self.phase == phase
    }

    pub fn has_completed_phase(
        &self,
        phase: AttackCruiserActorInvulnerabilityPhase,
        now: Instant,
    ) -> bool {
        self.has_phase(phase) && self.timer.time_until_next_event(now).is_zero()
    }

    pub fn set_phase(
        &mut self,
        phase: AttackCruiserActorInvulnerabilityPhase,
        duration: Duration,
        now: Instant,
    ) {
        self.phase = phase;
        self.timer.schedule_event(duration, now);
    }

    pub fn vulnerable(&self) -> bool {
        self.has_phase(AttackCruiserActorInvulnerabilityPhase::Vulnerable)
    }

    pub fn set_vulnerable(&mut self) {
        self.phase = AttackCruiserActorInvulnerabilityPhase::Vulnerable;
        self.timer.schedule_event(Duration::ZERO, Instant::now());
    }

    pub fn time_remaining(&self, now: Instant) -> Duration {
        self.timer.time_until_next_event(now)
    }

    pub fn paused(&self) -> bool {
        self.timer.paused()
    }

    pub fn pause_or_resume(&mut self, pause: bool) {
        self.timer.pause_or_resume(pause);
    }
}

#[derive(Clone, Debug)]
struct AttackCruiserActorStun {
    stunned: bool,
    timer: MinigameCountdown,
}

impl Default for AttackCruiserActorStun {
    fn default() -> Self {
        AttackCruiserActorStun {
            stunned: false,
            timer: MinigameCountdown::new(),
        }
    }
}

impl AttackCruiserActorStun {
    pub fn stunned(&self) -> bool {
        self.stunned
    }

    pub fn stun(&mut self, duration: Duration, now: Instant) {
        if duration.is_zero() {
            return;
        }

        self.stunned = true;
        self.timer.schedule_event(duration, now);
    }

    pub fn remove_stun(&mut self) {
        self.stunned = false;
        self.timer.schedule_event(Duration::ZERO, Instant::now());
    }

    pub fn time_remaining(&self, now: Instant) -> Duration {
        self.timer.time_until_next_event(now)
    }

    pub fn pause_or_resume(&mut self, pause: bool) {
        self.timer.pause_or_resume(pause);
    }
}

#[derive(Clone, Debug)]
struct AttackCruiserActor {
    pub id: i32,
    pub ship: Arc<AttackCruiserShipConfig>,
    pub pos: Pos3,
    pub yaw: f32,
    pub speed: Pos3,
    pub angular_speed: f32,
    pub forward_multiplier: f32,
    pub turn_multiplier: f32,
    pub health: u16,
    pub bvh: Option<Arc<Bvh>>,
    pub invulnerability: AttackCruiserActorInvulnerability,
    pub stun: AttackCruiserActorStun,
    pub primary_weapon_tier: usize,
    pub primary_weapon_projectile_last_used: Vec<Option<Instant>>,
    pub primary_weapon_actor_last_used: Vec<Option<Instant>>,
    pub spawn_time: Instant,
}

impl AttackCruiserActor {
    pub fn new(
        id: i32,
        pos: Pos3,
        yaw: f32,
        speed: f32,
        angular_speed: f32,
        bvh: Option<Arc<Bvh>>,
        ship: Arc<AttackCruiserShipConfig>,
        now: Instant,
    ) -> Self {
        let mut actor = AttackCruiserActor {
            id,
            pos,
            yaw,
            speed: Pos3 {
                x: yaw.sin() * speed,
                y: 0.0,
                z: yaw.cos() * speed,
            },
            angular_speed,
            forward_multiplier: zero_nan(speed / ship.max_speed),
            turn_multiplier: zero_nan(angular_speed / ship.max_angular_speed.to_radians()),
            health: ship.max_health,
            bvh,
            invulnerability: AttackCruiserActorInvulnerability::default(),
            stun: AttackCruiserActorStun::default(),
            primary_weapon_tier: 0,
            primary_weapon_projectile_last_used: Vec::new(),
            primary_weapon_actor_last_used: Vec::new(),
            ship,
            spawn_time: now,
        };

        actor.set_primary_weapon_tier(0);

        actor
    }

    pub fn seekable(&self) -> bool {
        !self.dead()
    }

    pub fn vulnerable(&self) -> bool {
        self.invulnerability.vulnerable()
    }

    pub fn dead(&self) -> bool {
        self.health == 0
    }

    pub fn expired(&self, now: Instant) -> bool {
        match self.ship.lifetime_millis {
            Some(lifetime_millis) => {
                self.spawn_time + Duration::from_millis(lifetime_millis.into()) < now
            }
            None => false,
        }
    }

    pub fn paused(&self) -> bool {
        self.invulnerability.paused()
    }

    pub fn pause_or_resume(&mut self, pause: bool) {
        self.invulnerability.pause_or_resume(pause);
        self.stun.pause_or_resume(pause);
    }

    pub fn add_health(&mut self, delta_health: i16, now: Instant) {
        if self.dead() {
            return;
        }

        if self.vulnerable() || delta_health > 0 {
            self.health = self
                .health
                .saturating_add_signed(delta_health)
                .min(self.ship.max_health);

            if self.dead() {
                let death_secs: f32 = self
                    .ship
                    .animations
                    .iter()
                    .filter(|animation| animation.animation_type.is_death())
                    .map(|animation| animation.duration_seconds)
                    .fold(0.0, |a, b| a.max(b));
                self.invulnerability.set_phase(
                    AttackCruiserActorInvulnerabilityPhase::Dead,
                    Duration::from_secs_f32(death_secs),
                    now,
                );
            }
        }
    }

    pub fn add_primary_weapon_tiers(&mut self, tiers: i16) {
        self.set_primary_weapon_tier(
            self.primary_weapon_tier
                .saturating_add_signed(tiers.into())
                .min(self.ship.weapons.primary_tiers.len().saturating_sub(1)),
        );
    }

    pub fn set_primary_weapon_tier(&mut self, new_tier: usize) {
        let is_initialized = !self.primary_weapon_projectile_last_used.is_empty()
            || !self.primary_weapon_actor_last_used.is_empty();
        if self.primary_weapon_tier == new_tier && is_initialized {
            return;
        }

        if new_tier < self.primary_weapon_tier && !self.vulnerable() {
            return;
        }

        self.primary_weapon_tier = new_tier;
        self.primary_weapon_projectile_last_used.clear();
        self.primary_weapon_actor_last_used.clear();
    }

    pub fn use_invulnerable_powerup(&mut self, invulnerability_duration: Duration, now: Instant) {
        if invulnerability_duration.is_zero() {
            return;
        }

        self.invulnerability.set_phase(
            AttackCruiserActorInvulnerabilityPhase::UsedPowerup,
            self.invulnerability
                .time_remaining(now)
                .saturating_add(invulnerability_duration),
            now,
        );
    }

    pub fn used_invulnerable_powerup(&self) -> bool {
        self.invulnerability
            .has_phase(AttackCruiserActorInvulnerabilityPhase::UsedPowerup)
    }

    pub fn completed_invulnerable_powerup(&self, now: Instant) -> bool {
        self.invulnerability
            .has_completed_phase(AttackCruiserActorInvulnerabilityPhase::UsedPowerup, now)
    }

    pub fn completed_death(&self, now: Instant) -> bool {
        self.dead()
            && self
                .invulnerability
                .has_completed_phase(AttackCruiserActorInvulnerabilityPhase::Dead, now)
    }

    pub fn respawn(&mut self, invulnerability_duration: Duration, now: Instant) {
        self.health = self.ship.max_health;
        self.invulnerability.set_phase(
            AttackCruiserActorInvulnerabilityPhase::Respawning,
            invulnerability_duration,
            now,
        );
    }

    pub fn respawning(&self) -> bool {
        self.invulnerability
            .has_phase(AttackCruiserActorInvulnerabilityPhase::Respawning)
    }

    pub fn completed_respawn(&self, now: Instant) -> bool {
        !self.dead()
            && self
                .invulnerability
                .has_completed_phase(AttackCruiserActorInvulnerabilityPhase::Respawning, now)
    }

    pub fn stun(&mut self, stun_duration: Duration, now: Instant) {
        if self.vulnerable() && self.ship.stunnable {
            // Adding to existing stun time is too overpowered, so always replace the stun time
            self.stun.stun(stun_duration, now);
        }
    }

    pub fn stunned(&self) -> bool {
        self.stun.stunned()
    }

    pub fn remove_stun(&mut self) {
        self.stun.remove_stun();
    }

    pub fn completed_stun(&self, now: Instant) -> bool {
        self.stun.stunned() && self.stun.time_remaining(now).is_zero()
    }

    pub fn set_vulnerable(&mut self) {
        self.invulnerability.set_vulnerable();
    }

    pub fn disabled(&self) -> bool {
        self.dead() || self.respawning() || self.stunned() || self.paused()
    }

    pub fn current_ai_behavior(&self) -> &AttackCruiserShipAiBehavior {
        for rule in &self.ship.ai_states {
            if self.matches_condition(&rule.condition) {
                return &rule.behavior;
            }
        }
        &self.ship.default_ai_behavior
    }

    pub fn hostility(
        &self,
        faction_configs: &HashMap<String, AttackCruiserFactionConfig>,
    ) -> AttackCruiserHostility {
        if !self.ship.show_arrow_to_player {
            return AttackCruiserHostility::Neutral;
        }

        let mut hostility = AttackCruiserHostility::Neutral;
        for faction in self.ship.self_factions.iter() {
            let Some(faction_config) = faction_configs.get(faction) else {
                continue;
            };

            hostility = match (hostility, faction_config.hostility) {
                (AttackCruiserHostility::Neutral, _) => faction_config.hostility,
                (_, AttackCruiserHostility::Neutral) => hostility,
                (AttackCruiserHostility::Friendly, AttackCruiserHostility::Hostile) => {
                    AttackCruiserHostility::Hostile
                }
                (AttackCruiserHostility::Hostile, AttackCruiserHostility::Friendly) => {
                    AttackCruiserHostility::Hostile
                }
                _ => hostility,
            };
        }

        hostility
    }

    pub fn attack_primary(
        &mut self,
        now: Instant,
        max_cooldown_error: Duration,
        target_pos: Pos3,
        target_speed: Pos3,
    ) -> (
        impl Iterator<Item = (&Arc<AttackCruiserProjectileConfig>, Pos3)> + use<'_>,
        impl Iterator<Item = (&AttackCruiserLaunchedShipConfig, Pos3)>,
    ) {
        let self_pos = self.pos;
        let yaw = self.yaw;

        let projectiles_opt = self
            .ship
            .weapons
            .primary_tiers
            .get(self.primary_weapon_tier)
            .map(|weapon| &weapon.projectiles);

        self.primary_weapon_projectile_last_used.resize(
            projectiles_opt
                .map(|projectiles| projectiles.len())
                .unwrap_or_default(),
            None,
        );
        let last_used_slice = &mut self.primary_weapon_projectile_last_used;

        let projectiles = projectiles_opt
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(move |(index, projectile)| {
                if let Some(last_used_opt) = last_used_slice.get_mut(index) {
                    if let Some(last_used) = last_used_opt {
                        let max_cooldown_error = projectile
                            .max_cooldown_error_millis
                            .map(|millis| Duration::from_millis(millis.into()))
                            .unwrap_or(max_cooldown_error);

                        let adjusted_cooldown =
                            Duration::from_millis(projectile.cooldown_millis.into())
                                .saturating_sub(max_cooldown_error);
                        let is_on_cooldown =
                            now.saturating_duration_since(*last_used) < adjusted_cooldown;

                        if is_on_cooldown {
                            return None;
                        }
                    }

                    let secs_to_intercept = match Self::calculate_time_to_intercept(
                        self_pos.x,
                        self_pos.z,
                        projectile.speed,
                        target_pos.x,
                        target_pos.z,
                        target_speed.x,
                        target_speed.z,
                    ) {
                        Some(time) => time,
                        None => {
                            let to_target_x = target_pos.x - self_pos.x;
                            let to_target_z = target_pos.z - self_pos.z;
                            let target_speed_sq =
                                target_speed.x * target_speed.x + target_speed.z * target_speed.z;

                            if target_speed_sq > TIME_EPSILON {
                                let dot_product =
                                    to_target_x * target_speed.x + to_target_z * target_speed.z;
                                (-dot_product / target_speed_sq).max(0.0)
                            } else {
                                0.0
                            }
                        }
                    };

                    let predicted_target_pos = target_pos + target_speed * secs_to_intercept;

                    let direction = direction(
                        Pos {
                            x: self_pos.x,
                            y: self_pos.y,
                            z: self_pos.z,
                            w: 0.0,
                        },
                        Pos {
                            x: predicted_target_pos.x,
                            y: predicted_target_pos.y,
                            z: predicted_target_pos.z,
                            w: 0.0,
                        },
                    );
                    return Self::validate_attack_angle(
                        projectile,
                        direction,
                        last_used_opt,
                        yaw,
                        projectile.min_launch_angle,
                        projectile.max_launch_angle,
                        now,
                    );
                }
                None
            });

        let actors_opt = self
            .ship
            .weapons
            .primary_tiers
            .get(self.primary_weapon_tier)
            .map(|weapon| &weapon.ships);

        self.primary_weapon_actor_last_used.resize(
            actors_opt.map(|actors| actors.len()).unwrap_or_default(),
            None,
        );
        let last_used_slice = &mut self.primary_weapon_actor_last_used;

        let actors =
            actors_opt
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(move |(index, actor)| {
                    if let Some(last_used_opt) = last_used_slice.get_mut(index) {
                        if let Some(last_used) = last_used_opt {
                            let max_cooldown_error = actor
                                .max_cooldown_error_millis
                                .map(|millis| Duration::from_millis(millis.into()))
                                .unwrap_or(max_cooldown_error);

                            let adjusted_cooldown =
                                Duration::from_millis(actor.cooldown_millis.into())
                                    .saturating_sub(max_cooldown_error);
                            let is_on_cooldown =
                                now.saturating_duration_since(*last_used) < adjusted_cooldown;

                            if is_on_cooldown {
                                return None;
                            }
                        }

                        let direction = direction(
                            Pos {
                                x: self_pos.x,
                                y: self_pos.y,
                                z: self_pos.z,
                                w: 0.0,
                            },
                            Pos {
                                x: target_pos.x,
                                y: target_pos.y,
                                z: target_pos.z,
                                w: 0.0,
                            },
                        );
                        return Self::validate_attack_angle(
                            actor,
                            direction,
                            last_used_opt,
                            yaw,
                            actor.min_launch_angle,
                            actor.max_launch_angle,
                            now,
                        );
                    }
                    None
                });

        (projectiles, actors)
    }

    pub fn seek_target(&mut self, target_pos: Pos3, target_speed: Pos3, delta_secs: f32) {
        if delta_secs <= 0.0 {
            return;
        }

        let speed = (self.speed.x.powi(2) + self.speed.z.powi(2)).sqrt();

        if self.disabled() {
            let new_speed = speed - self.ship.deceleration * delta_secs;
            let scaling_factor = (new_speed / speed).max(0.0);
            self.speed.x *= scaling_factor;
            self.speed.z *= scaling_factor;
            self.pos.x += self.speed.x * delta_secs;
            self.pos.z += self.speed.z * delta_secs;

            // Even though it doesn't physically make sense, the client's interpolation
            // sets angular speed to 0 on death. If we don't match that on server, dead
            // actors appear to shake and teleport
            self.angular_speed = 0.0;
            self.yaw = normalize_angle(self.yaw + self.angular_speed * delta_secs);

            return;
        }

        let secs_to_intercept = Self::calculate_time_to_intercept(
            self.pos.x,
            self.pos.z,
            speed,
            target_pos.x,
            target_pos.z,
            target_speed.x,
            target_speed.z,
        );
        let predicted_target_x = target_pos.x + target_speed.x * secs_to_intercept.unwrap_or(0.0);
        let predicted_target_z = target_pos.z + target_speed.z * secs_to_intercept.unwrap_or(0.0);

        let delta_x = predicted_target_x - self.pos.x;
        let delta_z = predicted_target_z - self.pos.z;

        // Avoid large directional swings for small positional changes
        let desired_yaw = if delta_x.abs() < 1.0 && delta_z.abs() < 1.0 {
            self.yaw
        } else {
            delta_x.atan2(delta_z)
        };
        let delta_yaw = normalize_angle(desired_yaw - self.yaw);

        let max_angular_acceleration = self.ship.angular_acceleration.to_radians();
        let max_angular_deceleration = self.ship.angular_deceleration.to_radians();
        let max_angular_speed = self.ship.max_angular_speed.to_radians();
        let max_delta_yaw = max_angular_speed * delta_secs;

        let new_angular_speed = {
            // Brake half a frame early to avoid large spikes in angular speed when delta_yaw is small
            let angular_braking_distance = (delta_yaw.abs() - (max_delta_yaw * 0.5)).max(0.0);
            // v^2 = 2ad
            let max_safe_angular_speed =
                (2.0 * max_angular_deceleration * angular_braking_distance).sqrt();

            let target_direction = if delta_yaw == 0.0 {
                0.0
            } else {
                delta_yaw.signum()
            };
            let desired_angular_speed = (target_direction * max_safe_angular_speed)
                .clamp(-max_angular_speed, max_angular_speed);

            let delta_angular_speed = desired_angular_speed - self.angular_speed;

            let change_angular_direction =
                delta_angular_speed.is_sign_negative() != self.angular_speed.is_sign_negative();
            let angular_acceleration = if change_angular_direction {
                max_angular_deceleration
            } else {
                max_angular_acceleration
            } * delta_angular_speed.signum();

            let mut new_angular_speed = self.angular_speed + angular_acceleration * delta_secs;

            // Snap angular speed if beyond desired speed
            let error_sign_changed = delta_angular_speed.is_sign_negative()
                != (desired_angular_speed - new_angular_speed).is_sign_negative();
            if delta_angular_speed == 0.0 || error_sign_changed {
                new_angular_speed = desired_angular_speed;
            }

            new_angular_speed
        };

        let new_yaw = normalize_angle(self.yaw + new_angular_speed * delta_secs);

        let accelerated_speed =
            (speed + self.ship.acceleration * delta_secs).min(self.ship.max_speed);
        self.speed.x = new_yaw.sin() * accelerated_speed;
        self.speed.z = new_yaw.cos() * accelerated_speed;

        self.yaw = new_yaw;
        self.angular_speed = new_angular_speed;
        self.turn_multiplier = zero_nan(new_angular_speed / max_angular_speed);

        self.pos.x += self.speed.x * delta_secs;
        self.pos.z += self.speed.z * delta_secs;
    }

    fn matches_condition(&self, expr: &AttackCruiserShipBoolExpr) -> bool {
        match expr {
            AttackCruiserShipBoolExpr::Condition(prop) => match prop {
                AttackCruiserShipPropertyExpr::HealthPercent(op, value) => {
                    let current_pct = self.health as f32 / self.ship.max_health as f32;
                    op.eval(current_pct, *value)
                }
                AttackCruiserShipPropertyExpr::LifetimeMillis(op, value) => {
                    let lifetime_millis = self.spawn_time.elapsed().as_millis();
                    op.eval(lifetime_millis, *value as u128)
                }
                AttackCruiserShipPropertyExpr::ProbabilityLessThan(chance) => {
                    let roll: f32 = rand::thread_rng().gen();
                    roll < *chance
                }
            },
            AttackCruiserShipBoolExpr::Not(inner) => !self.matches_condition(inner),
            AttackCruiserShipBoolExpr::And(expressions) => {
                expressions.iter().all(|e| self.matches_condition(e))
            }
            AttackCruiserShipBoolExpr::Or(expressions) => {
                expressions.iter().any(|e| self.matches_condition(e))
            }
        }
    }

    fn calculate_time_to_intercept(
        pos_x: f32,
        pos_z: f32,
        speed: f32,
        target_x: f32,
        target_z: f32,
        target_speed_x: f32,
        target_speed_z: f32,
    ) -> Option<f32> {
        let to_target_x = target_x - pos_x;
        let to_target_z = target_z - pos_z;

        // Quadratic terms derived from the vector intersection equation:
        // (to_target_x + target_speed_x * t)^2 + (to_target_z + target_speed_z * t)^2 = (speed * t)^2
        let a_target_term = target_speed_x * target_speed_x + target_speed_z * target_speed_z;
        let a = a_target_term - speed.powi(2);
        let b = 2.0 * (to_target_x * target_speed_x + to_target_z * target_speed_z);
        let c = to_target_x * to_target_x + to_target_z * to_target_z;

        let max_speed_sq = a_target_term.max(speed * speed);
        let a_epsilon = max_speed_sq * TIME_EPSILON;

        if a.abs() < a_epsilon {
            // Find the maximum 'b' could reach in this frame
            let target_speed_magnitude =
                (target_speed_x * target_speed_x + target_speed_z * target_speed_z).sqrt();
            let distance_magnitude = c.sqrt();
            let max_b = 2.0 * distance_magnitude * target_speed_magnitude;

            let b_epsilon = max_b * TIME_EPSILON;

            // If b is effectively zero relative to map scale, the paths are completely parallel/perpendicular
            if b.abs() < b_epsilon {
                return None;
            }

            let t = -c / b;
            return if t > TIME_EPSILON { Some(t) } else { None };
        }

        let discriminant = b * b - 4.0 * a * c;
        if discriminant < 0.0 {
            // Target is moving too fast to intercept
            return None;
        }

        let sqrt_discriminant = discriminant.sqrt();
        let time1 = (-b - sqrt_discriminant) / (2.0 * a);
        let time2 = (-b + sqrt_discriminant) / (2.0 * a);

        match (time1 > TIME_EPSILON, time2 > TIME_EPSILON) {
            (true, true) => Some(time1.min(time2)),
            (true, false) => Some(time1),
            (false, true) => Some(time2),
            (false, false) => None,
        }
    }

    fn validate_attack_angle<T>(
        value: T,
        direction: Pos,
        last_used_opt: &mut Option<Instant>,
        yaw: f32,
        min_launch_angle: Angle,
        max_launch_angle: Angle,
        now: Instant,
    ) -> Option<(T, Pos3)> {
        let attack_angle = direction.x.atan2(direction.z);

        let min_angle = yaw + min_launch_angle.to_radians();
        let max_angle = yaw + max_launch_angle.to_radians();
        let allows_complete_circle = (max_angle - min_angle).abs() >= 2.0 * PI;
        if !allows_complete_circle {
            let allowed_sector_width = normalize_angle_positive(max_angle - min_angle);
            let relative_attack_angle = normalize_angle_positive(attack_angle - min_angle);

            if relative_attack_angle > allowed_sector_width {
                return None;
            }
        }

        *last_used_opt = Some(now);
        Some((value, direction.into()))
    }
}

struct AttackCruiserPendingActor {
    actor: AttackCruiserActor,
    ship_name: String,
}

impl AttackCruiserPendingActor {
    pub fn new(
        pos: Pos3,
        yaw: f32,
        speed: f32,
        angular_speed: f32,
        bvh: Option<Arc<Bvh>>,
        ship: Arc<AttackCruiserShipConfig>,
        ship_name: String,
        now: Instant,
    ) -> Self {
        AttackCruiserPendingActor {
            actor: AttackCruiserActor::new(0, pos, yaw, speed, angular_speed, bvh, ship, now),
            ship_name,
        }
    }

    pub fn ship(&self) -> &Arc<AttackCruiserShipConfig> {
        &self.actor.ship
    }

    pub fn ship_name(&self) -> &String {
        &self.ship_name
    }

    pub fn finalize(mut self, id: i32) -> (AttackCruiserActor, String) {
        self.actor.id = id;
        (self.actor, self.ship_name)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AttackCruiserPlayerBoundsPhase {
    #[default]
    Inside,
    Outside,
    OutsideWaitingToWarp,
}

#[derive(Clone, Debug)]
struct AttackCruiserPlayerBounds {
    phase: AttackCruiserPlayerBoundsPhase,
    timer: MinigameCountdown,
}

impl Default for AttackCruiserPlayerBounds {
    fn default() -> Self {
        AttackCruiserPlayerBounds {
            phase: AttackCruiserPlayerBoundsPhase::default(),
            timer: MinigameCountdown::new(),
        }
    }
}

impl AttackCruiserPlayerBounds {
    pub fn phase(&self) -> AttackCruiserPlayerBoundsPhase {
        self.phase
    }

    pub fn has_phase(&self, phase: AttackCruiserPlayerBoundsPhase) -> bool {
        self.phase == phase
    }

    pub fn has_completed_phase(&self, now: Instant) -> bool {
        self.timer.time_until_next_event(now).is_zero()
    }

    pub fn set_phase(
        &mut self,
        phase: AttackCruiserPlayerBoundsPhase,
        duration: Duration,
        now: Instant,
    ) {
        self.phase = phase;
        self.timer.schedule_event(duration, now);
    }

    pub fn set_in_bounds(&mut self) {
        self.phase = AttackCruiserPlayerBoundsPhase::Inside;
        self.timer.schedule_event(Duration::ZERO, Instant::now());
    }

    pub fn pause_or_resume(&mut self, pause: bool) {
        self.timer.pause_or_resume(pause);
    }
}

#[derive(Clone, Debug)]
struct AttackCruiserPlayer {
    pub guid: u32,
    pub ready: bool,
    pub actor: AttackCruiserActor,
    pub score: i32,
    pub score_multiplier_tier_progress: u16,
    pub score_multiplier_tier: u8,
    pub lives: u8,
    pub max_lives: u8,
    pub bounds: AttackCruiserPlayerBounds,
    pub bounds_warning_hud_timer: MinigameCountdown,
    pub damage_alarm_sound_timer: MinigameCountdown,
    pub secondary_item: String,
    pub secondary_item_count: u8,
}

impl AttackCruiserPlayer {
    pub fn new(
        guid: u32,
        actor_id: i32,
        ship: Arc<AttackCruiserShipConfig>,
        lives: u8,
        max_lives: u8,
        pos: Pos3,
        yaw: f32,
        bvh: Option<Arc<Bvh>>,
        now: Instant,
    ) -> Self {
        AttackCruiserPlayer {
            guid,
            ready: false,
            actor: AttackCruiserActor::new(actor_id, pos, yaw, 0.0, 0.0, bvh, ship, now),
            score: 0,
            score_multiplier_tier_progress: 0,
            score_multiplier_tier: 1,
            lives,
            max_lives,
            bounds: AttackCruiserPlayerBounds::default(),
            bounds_warning_hud_timer: MinigameCountdown::new(),
            damage_alarm_sound_timer: MinigameCountdown::new(),
            secondary_item: "".to_string(),
            secondary_item_count: 0,
        }
    }

    pub fn pause_or_resume(&mut self, pause: bool) {
        self.actor.pause_or_resume(pause);
        self.bounds_warning_hud_timer.pause_or_resume(pause);
        self.damage_alarm_sound_timer.pause_or_resume(pause);
        self.bounds.pause_or_resume(pause);
    }

    pub fn respawnable(&self, now: Instant) -> bool {
        self.has_lives() && self.completed_death(now)
    }

    pub fn has_lives(&self) -> bool {
        self.lives > 0
    }

    pub fn lost(&self, now: Instant) -> bool {
        (!self.has_lives() && self.completed_death(now)) || self.actor.expired(now)
    }

    pub fn respawn(&mut self, invulnerability_duration: Duration, now: Instant) {
        self.actor.respawn(invulnerability_duration, now);
    }

    pub fn completed_respawn(&self, now: Instant) -> bool {
        self.actor.completed_respawn(now)
    }

    pub fn complete_respawn(&mut self) {
        self.actor.set_vulnerable()
    }

    pub fn dead(&self) -> bool {
        self.actor.dead() || !self.has_lives()
    }

    pub fn respawning(&self) -> bool {
        self.actor.respawning()
    }

    pub fn warped_away(&self) -> bool {
        self.bounds
            .has_phase(AttackCruiserPlayerBoundsPhase::Outside)
    }

    pub fn seekable(&self) -> bool {
        self.actor.seekable() && !self.warped_away()
    }

    pub fn add_health(&mut self, delta_health: i16, now: Instant) {
        if !self.dead() {
            self.actor.add_health(delta_health, now);

            if self.actor.dead() {
                self.lives = self.lives.saturating_sub(1);
            }
        }
    }

    pub fn add_lives(&mut self, lives: i8) {
        if self.actor.vulnerable() || lives > 0 {
            self.set_lives_unchecked(self.lives.saturating_add_signed(lives));
        }
    }

    pub fn set_lives_unchecked(&mut self, lives: u8) {
        self.lives = lives.min(self.max_lives);
    }

    pub fn add_primary_tiers(&mut self, tiers: i16) {
        self.actor.add_primary_weapon_tiers(tiers);
    }

    pub fn add_secondary_item(&mut self, item: &String, delta_count: i8) {
        if &self.secondary_item != item {
            self.secondary_item_count = 0;
        }

        self.secondary_item = item.clone();
        self.secondary_item_count = self.secondary_item_count.saturating_add_signed(delta_count);
    }

    pub fn use_secondary_item(&mut self) -> Option<&String> {
        if self.secondary_item_count == 0 {
            return None;
        }

        self.secondary_item_count -= 1;
        Some(&self.secondary_item)
    }

    pub fn use_invulnerable_powerup(&mut self, invulnerability_duration: Duration, now: Instant) {
        self.actor
            .use_invulnerable_powerup(invulnerability_duration, now);
    }

    pub fn used_invulnerable_powerup(&self) -> bool {
        self.actor.used_invulnerable_powerup()
    }

    pub fn completed_invulnerable_powerup(&self, now: Instant) -> bool {
        self.actor.completed_invulnerable_powerup(now)
    }

    pub fn complete_invulnerable_powerup(&mut self) {
        self.actor.set_vulnerable();
    }

    pub fn stun(&mut self, stun_duration: Duration, now: Instant) {
        self.actor.stun(stun_duration, now);
    }

    pub fn stunned(&self) -> bool {
        self.actor.stunned()
    }

    pub fn remove_stun(&mut self) {
        self.actor.remove_stun();
    }

    pub fn completed_stun(&self, now: Instant) -> bool {
        self.actor.completed_stun(now)
    }

    pub fn paused(&self) -> bool {
        self.actor.paused()
    }

    pub fn disabled(&self) -> bool {
        self.dead() || self.respawning() || self.stunned() || self.paused()
    }

    pub fn disarmed(&self) -> bool {
        self.disabled()
            || self
                .bounds
                .has_phase(AttackCruiserPlayerBoundsPhase::Outside)
    }

    fn completed_death(&self, now: Instant) -> bool {
        self.actor.completed_death(now)
    }
}

#[derive(Clone, Debug)]
enum AttackCruiserGameState {
    WaitingForPlayersReady,
    WaveActive,
    GameOver,
}

const fn default_yaw() -> Angle {
    Angle::Degrees(0.0)
}

const fn default_wobble() -> Angle {
    Angle::Degrees(3.0)
}

const fn default_speed() -> f32 {
    500.0
}

const fn default_lifetime_millis() -> u16 {
    3000
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

const fn default_min_launch_angle() -> Angle {
    Angle::Degrees(0.0)
}

const fn default_max_launch_angle() -> Angle {
    Angle::Degrees(360.0)
}

const fn default_screen_relative_turning() -> bool {
    true
}

const fn default_wipe_style() -> AttackCruiserCinematicStyle {
    AttackCruiserCinematicStyle::Random
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserCameraConfig {
    default_distance: f32,
    min_distance: f32,
    max_distance: f32,
    pitch: Angle,
    #[serde(default)]
    offset_z: f32,
    target_tracking_high_level_quotient: f32,
    zoom_step_quantization: f32,
    zoom_step_high_level_quotient: f32,
    near_clip_distance: f32,
    #[serde(default = "default_screen_relative_turning")]
    screen_relative_turning: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
enum AttackCruiserChallengeConfig {
    #[default]
    None,
    Timed {
        limit_seconds: u32,
    },
    ScoreTarget {
        score: u32,
    },
}

impl From<&AttackCruiserChallengeConfig> for AttackCruiserChallengeMode {
    fn from(value: &AttackCruiserChallengeConfig) -> Self {
        match value {
            AttackCruiserChallengeConfig::None => AttackCruiserChallengeMode::None,
            AttackCruiserChallengeConfig::Timed { .. } => AttackCruiserChallengeMode::Timed,
            AttackCruiserChallengeConfig::ScoreTarget { .. } => {
                AttackCruiserChallengeMode::ScoreTarget
            }
        }
    }
}

impl AttackCruiserChallengeConfig {
    pub fn value(&self) -> u32 {
        match self {
            AttackCruiserChallengeConfig::None => 0,
            AttackCruiserChallengeConfig::Timed { limit_seconds } => *limit_seconds,
            AttackCruiserChallengeConfig::ScoreTarget { score } => *score,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserFactionConfig {
    hostility: AttackCruiserHostility,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserHealthBarConfig {
    foreground_image_id: u32,
    background_image_id: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserIntroActorConfig {
    model_id: u32,
    animation_id: i32,
}

impl From<&AttackCruiserIntroActorConfig> for AttackCruiserEventActorConfig {
    fn from(value: &AttackCruiserIntroActorConfig) -> Self {
        AttackCruiserEventActorConfig {
            model_id: value.model_id,
            animation_id: value.animation_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserIntroCinematicConfig {
    duration_seconds: f32,
    camera_animation_id: i32,
    camera_heading: Angle,
    camera_fov: Angle,
    #[serde(default)]
    flip_camera_z: bool,
    #[serde(default = "default_wipe_style")]
    pre_wipe_style: AttackCruiserCinematicStyle,
    #[serde(default = "default_wipe_style")]
    post_wipe_style: AttackCruiserCinematicStyle,
}

impl From<&AttackCruiserIntroCinematicConfig> for AttackCruiserEventCinematicConfig {
    fn from(value: &AttackCruiserIntroCinematicConfig) -> Self {
        AttackCruiserEventCinematicConfig {
            duration_seconds: value.duration_seconds,
            camera_animation_id: value.camera_animation_id,
            camera_heading_degrees: value.camera_heading.to_degrees(),
            camera_fov_degrees: value.camera_fov.to_degrees(),
            flip_camera_z: AttackCruiserBool(value.flip_camera_z),
            pre_wipe_style: value.pre_wipe_style,
            post_wipe_style: value.post_wipe_style,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserIntroConfig {
    #[serde(default)]
    ships: Vec<AttackCruiserIntroActorConfig>,
    #[serde(default)]
    cinematics: Vec<AttackCruiserIntroCinematicConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlanetConfig {
    model_id: u32,
    center: Pos3,
    angular_speed: Angle,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserActorSecondaryItemDeltaConfig {
    name: String,
    count: i8,
}

impl AttackCruiserActorSecondaryItemDeltaConfig {
    fn saturating_add(&self, rhs: &Self) -> Self {
        AttackCruiserActorSecondaryItemDeltaConfig {
            name: rhs.name.clone(),
            count: match self.name == rhs.name {
                true => self.count.saturating_add(rhs.count),
                false => rhs.count,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserActorDeltas {
    #[serde(default)]
    health: i16,
    #[serde(default)]
    primary_tiers: i16,
    #[serde(default)]
    player_lives: i8,
    secondary_item: Option<AttackCruiserActorSecondaryItemDeltaConfig>,
    #[serde(default)]
    stun_millis: u32,
}

impl AttackCruiserActorDeltas {
    fn saturating_add(&self, rhs: &Self) -> Self {
        AttackCruiserActorDeltas {
            health: self.health.saturating_add(rhs.health),
            primary_tiers: self.primary_tiers.saturating_add(rhs.primary_tiers),
            player_lives: self.player_lives.saturating_add(rhs.player_lives),
            secondary_item: match (&self.secondary_item, &rhs.secondary_item) {
                (None, None) => None,
                (None, Some(rhs_item)) => Some(rhs_item.clone()),
                (Some(lhs_item), None) => Some(lhs_item.clone()),
                (Some(lhs_item), Some(rhs_item)) => Some(lhs_item.saturating_add(rhs_item)),
            },
            stun_millis: self.stun_millis.saturating_add(rhs.stun_millis),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserProjectileConfig {
    composite_effect_id: Option<u32>,
    hit_composite_effect_id: u32,
    #[serde(default = "default_yaw")]
    yaw: Angle,
    #[serde(default = "default_wobble")]
    wobble: Angle,
    #[serde(default = "default_speed")]
    speed: f32,
    cooldown_millis: u16,
    max_cooldown_error_millis: Option<u16>,
    #[serde(default = "default_lifetime_millis")]
    lifetime_millis: u16,
    #[serde(default = "default_count")]
    count: u8,
    #[serde(default = "default_launch_offset")]
    launch_offset: f32,
    #[serde(default = "default_launch_height")]
    launch_height: f32,
    #[serde(default = "default_min_launch_angle")]
    min_launch_angle: Angle,
    #[serde(default = "default_max_launch_angle")]
    max_launch_angle: Angle,
    length: f32,
    target_factions: HashSet<String>,
    #[serde(default)]
    target_deltas: AttackCruiserActorDeltas,
    #[serde(default)]
    self_deltas: AttackCruiserActorDeltas,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserSpawnedShipConfig {
    ship: String,
    #[serde(default = "default_yaw")]
    yaw: Angle,
    #[serde(default = "default_wobble")]
    wobble: Angle,
    #[serde(default = "default_count")]
    count: u8,
    #[serde(default = "default_launch_offset")]
    launch_offset: f32,
    #[serde(default = "default_launch_height")]
    launch_height: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserLaunchedShipConfig {
    ship: String,
    #[serde(default = "default_yaw")]
    yaw: Angle,
    #[serde(default = "default_wobble")]
    wobble: Angle,
    cooldown_millis: u16,
    max_cooldown_error_millis: Option<u16>,
    #[serde(default = "default_count")]
    count: u8,
    #[serde(default = "default_launch_offset")]
    launch_offset: f32,
    #[serde(default = "default_launch_height")]
    launch_height: f32,
    #[serde(default = "default_min_launch_angle")]
    min_launch_angle: Angle,
    #[serde(default = "default_max_launch_angle")]
    max_launch_angle: Angle,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPrimaryWeaponConfig {
    #[serde(default)]
    projectiles: Vec<Arc<AttackCruiserProjectileConfig>>,
    #[serde(default)]
    ships: Vec<AttackCruiserLaunchedShipConfig>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserWeaponConfig {
    primary_tiers: Vec<AttackCruiserPrimaryWeaponConfig>,
}

impl AttackCruiserWeaponConfig {
    pub fn cooldown_millis(&self) -> u16 {
        self.primary_tiers
            .iter()
            .flat_map(|tier| {
                tier.projectiles
                    .iter()
                    .map(|projectile| projectile.cooldown_millis)
                    .chain(tier.ships.iter().map(|actor| actor.cooldown_millis))
            })
            .reduce(gcd_u16)
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserShipAnimationConfig {
    animation_type: AttackCruiserActorAnimationType,
    animation_id: i32,
    duration_seconds: f32,
}

impl From<&AttackCruiserShipAnimationConfig> for AttackCruiserActorAnimationConfig {
    fn from(value: &AttackCruiserShipAnimationConfig) -> Self {
        AttackCruiserActorAnimationConfig {
            animation_type: value.animation_type,
            animation_id: value.animation_id,
            loops: AttackCruiserBool(false),
            duration_seconds: value.duration_seconds,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserShipCinematicConfig {
    cinematic_type: AttackCruiserActorCinematicType,
    camera_animation_id: i32,
    duration_seconds: f32,
}

impl From<&AttackCruiserShipCinematicConfig> for AttackCruiserActorCinematicConfig {
    fn from(value: &AttackCruiserShipCinematicConfig) -> Self {
        AttackCruiserActorCinematicConfig {
            cinematic_type: value.cinematic_type,
            duration_seconds: value.duration_seconds,
            camera_animation_id: value.camera_animation_id,
            pre_wipe_style: AttackCruiserCinematicStyle::None,
            post_wipe_style: AttackCruiserCinematicStyle::None,
            post_camera_ease_in_seconds: 0.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserShipDamageStateConfig {
    min_health_percent: f32,
    texture_alias: String,
    #[serde(default)]
    effects: Vec<AttackCruiserActorDamageStateEffectConfig>,
}

impl From<&AttackCruiserShipDamageStateConfig> for AttackCruiserActorDamageStateConfig {
    fn from(value: &AttackCruiserShipDamageStateConfig) -> Self {
        AttackCruiserActorDamageStateConfig {
            min_health_percent: value.min_health_percent,
            texture_alias: value.texture_alias.clone(),
            effects: AttackCruiserVec("".to_string(), value.effects.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
enum AttackCruiserShipOp {
    LessThan,
    LessOrEqual,
    Equal,
    GreaterOrEqual,
    GreaterThan,
}

impl AttackCruiserShipOp {
    fn eval<T: PartialOrd>(&self, left: T, right: T) -> bool {
        match self {
            AttackCruiserShipOp::LessThan => left < right,
            AttackCruiserShipOp::LessOrEqual => left <= right,
            AttackCruiserShipOp::Equal => left == right,
            AttackCruiserShipOp::GreaterOrEqual => left >= right,
            AttackCruiserShipOp::GreaterThan => left > right,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
enum AttackCruiserShipPropertyExpr {
    HealthPercent(AttackCruiserShipOp, f32),
    LifetimeMillis(AttackCruiserShipOp, u32),
    ProbabilityLessThan(f32),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
enum AttackCruiserShipBoolExpr {
    Condition(AttackCruiserShipPropertyExpr),
    Not(Box<AttackCruiserShipBoolExpr>),
    And(Vec<AttackCruiserShipBoolExpr>),
    Or(Vec<AttackCruiserShipBoolExpr>),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
enum AttackCruiserShipAiMovement {
    #[default]
    SeekTarget,
    RandomPos,
    FixedPos(Pos3),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserAoeConfig {
    radius: f32,
    target_factions: HashSet<String>,
    #[serde(default)]
    target_deltas: AttackCruiserActorDeltas,
    #[serde(default)]
    self_deltas: AttackCruiserActorDeltas,
    composite_effect_id: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserShipAiBehavior {
    movement: AttackCruiserShipAiMovement,
    aoe: Option<Arc<AttackCruiserAoeConfig>>,
    #[serde(default)]
    ships: Vec<AttackCruiserSpawnedShipConfig>,
    #[serde(default)]
    despawn: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserShipAiStateRule {
    condition: AttackCruiserShipBoolExpr,
    behavior: AttackCruiserShipAiBehavior,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserShipConfig {
    self_factions: HashSet<String>,
    seek_factions: HashSet<String>,
    #[serde(default = "default_true")]
    show_arrow_to_player: bool,
    max_alive: u16,
    model_id: u32,
    asset_name: Option<String>,
    #[serde(default)]
    enable_collision: bool,
    max_roll: Angle,
    max_speed: f32,
    acceleration: f32,
    deceleration: f32,
    max_angular_speed: Angle,
    angular_acceleration: Angle,
    angular_deceleration: Angle,
    #[serde(default)]
    stationary_turn: f32,
    max_health: u16,
    #[serde(default = "default_true")]
    stunnable: bool,
    overhead_health_scale: f32,
    thruster_effect_id: Option<u32>,
    invulnerable_effect_id: Option<u32>,
    stunned_effect_id: Option<u32>,
    death_start_effect_id: Option<u32>,
    death_end_effect_id: Option<u32>,
    despawn_effect_id: Option<u32>,
    lifetime_millis: Option<u32>,
    #[serde(default)]
    ships_on_death: Vec<AttackCruiserSpawnedShipConfig>,
    #[serde(default)]
    animations: Vec<AttackCruiserShipAnimationConfig>,
    #[serde(default)]
    cinematics: Vec<AttackCruiserShipCinematicConfig>,
    #[serde(default)]
    damage_states: Vec<AttackCruiserShipDamageStateConfig>,
    #[serde(default)]
    weapons: AttackCruiserWeaponConfig,
    #[serde(default)]
    ai_states: Vec<AttackCruiserShipAiStateRule>,
    #[serde(default)]
    default_ai_behavior: AttackCruiserShipAiBehavior,
}

static EMPTY_SHIP_CONFIG: LazyLock<Arc<AttackCruiserShipConfig>> =
    LazyLock::new(|| Arc::new(AttackCruiserShipConfig::default()));

fn ship_startup_config_name(ship_name: &String) -> String {
    format!("ship_{ship_name}")
}

fn ship_physics_startup_config_name(ship_name: &String) -> String {
    format!("ship_{ship_name}_physics")
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserSpawnLocation {
    pos: Pos3,
    yaw: Angle,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlayerSecondaryItemConfig {
    icon_id: i32,
    aoe: Option<Arc<AttackCruiserAoeConfig>>,
    #[serde(default)]
    ships: Vec<AttackCruiserSpawnedShipConfig>,
    #[serde(default)]
    invulnerability_millis: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlayerConfig {
    lives: u8,
    max_lives: u8,
    damage_alarm_sound_id: u32,
    damage_alarm_health_percent: f32,
    damage_alarm_interval_millis: u32,
    post_respawn_invulnerability_millis: u32,
    out_of_bounds_warp_millis: u32,
    out_of_bounds_warp_delay_millis: u32,
    #[serde(default)]
    secondary_items: HashMap<String, AttackCruiserPlayerSecondaryItemConfig>,
    spawn1: AttackCruiserSpawnLocation,
    spawn2: AttackCruiserSpawnLocation,
    ship: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackCruiserPlayfieldConfig {
    center: Pos3,
    radius_x: f32,
    radius_z: f32,
    warning_radius_ratio: f32,
    warning_message_id: u32,
    warning_millis: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCruiserConfig {
    actor_update_radius: f32,
    camera: AttackCruiserCameraConfig,
    client_actor_update_interval_millis: u16,
    #[serde(default)]
    challenge: AttackCruiserChallengeConfig,
    factions: HashMap<String, AttackCruiserFactionConfig>,
    health_bar: AttackCruiserHealthBarConfig,
    intro: AttackCruiserIntroConfig,
    max_weapon_cooldown_error_millis: u16,
    planet: AttackCruiserPlanetConfig,
    player: AttackCruiserPlayerConfig,
    playfield: AttackCruiserPlayfieldConfig,
    ships: HashMap<String, Arc<AttackCruiserShipConfig>>,
    sound_id: i32,
}

impl AttackCruiserConfig {
    fn ship(&self, ship_name: &String) -> Arc<AttackCruiserShipConfig> {
        self.ships
            .get(ship_name)
            .unwrap_or_else(|| {
                info!("Attack Cruiser has no ship {ship_name}. Defaulting to empty ship.");
                &EMPTY_SHIP_CONFIG
            })
            .clone()
    }

    fn validate_bvhs(&self, bvhs: &HashMap<String, Arc<Bvh>>) -> HashMap<String, Arc<Bvh>> {
        self.ships
            .iter()
            .filter_map(|(ship_name, ship)| {
                ship.asset_name.as_ref().and_then(|asset_name| {
                    let result = bvhs.get(asset_name)
                        .cloned()
                        .map(|bvh| (ship_name.clone(), bvh));

                    if result.is_none() {
                        info!("Attack Cruiser ship {ship_name} has no valid BVH {asset_name}. Defaulting to empty BVH.");
                    }

                    result
                })
            })
            .collect()
    }

    fn validate_ships(&self) {
        self.ships.values().for_each(|ship| {
            ship.self_factions.iter().for_each(|faction|  {
                if !self.factions.contains_key(faction) {
                    info!("Attack Cruiser ship self_factions references unknown faction {}", faction);
                }
            });
            ship.seek_factions.iter().for_each(|faction|  {
                if !self.factions.contains_key(faction) {
                    info!("Attack Cruiser ship seek_factions references unknown faction {}", faction);
                }
            });

            ship.weapons.primary_tiers.iter().for_each(|tier| tier.projectiles.iter().for_each(|projectile| {
                if let Some(secondary_item) = &projectile.self_deltas.secondary_item {
                    if !self.player.secondary_items.contains_key(&secondary_item.name) {
                        info!("Attack Cruiser ship projectile self_deltas references unknown secondary item {}", secondary_item.name);
                    }
                }

                if let Some(secondary_item) = &projectile.target_deltas.secondary_item {
                    if !self.player.secondary_items.contains_key(&secondary_item.name) {
                        info!("Attack Cruiser ship projectile target_deltas references unknown secondary item {}", secondary_item.name);
                    }
                }

                projectile.target_factions.iter().for_each(|target_faction| {
                    if !self.factions.contains_key(target_faction) {
                        info!("Attack Cruiser ship projectile target_factions references unknown faction {}", target_faction);
                    }
                });
            }));

            if let Some(aoe) = &ship.default_ai_behavior.aoe {
                aoe.target_factions.iter().for_each(|target_faction| {
                    if !self.factions.contains_key(target_faction) {
                        info!("Attack Cruiser ship AOE target_factions references unknown faction {}", target_faction);
                    }
                });
            }

            ship.ai_states.iter().for_each(|ai_state| {
                if let Some(aoe) = &ai_state.behavior.aoe {
                    aoe.target_factions.iter().for_each(|target_faction| {
                        if !self.factions.contains_key(target_faction) {
                            info!("Attack Cruiser ship AOE target_factions references unknown faction {}", target_faction);
                        }
                    });
                }
            });
        });
    }
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
    direction: Pos3,
    speed: Pos3,
    origin: Pos3,
    launch_time: Instant,
    config: Arc<AttackCruiserProjectileConfig>,
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
        rng: &mut ThreadRng,
        launched_by_actor_id: i32,
        actor_origin: Pos3,
        direction: Pos3,
        projectile: &Arc<AttackCruiserProjectileConfig>,
        now: Instant,
    ) -> Result<Vec<AttackCruiserProjectileSpawn>, ProcessPacketError> {
        let mut launched_projectiles = Vec::new();

        for _ in 0..projectile.count {
            let projectile_id = self.next_id()?;

            let launch_location = launch_vector(
                rng,
                actor_origin,
                direction,
                projectile.speed,
                projectile.wobble,
                projectile.yaw,
                projectile.launch_offset,
                projectile.launch_height,
            );

            let expiry_time = now
                .checked_add(Duration::from_millis(projectile.lifetime_millis.into()))
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
                    direction: launch_location.direction,
                    speed: launch_location.speed,
                    origin: launch_location.origin,
                    launch_time: now,
                    config: projectile.clone(),
                },
            );
            self.expiry.push(projectile_id, Reverse(expiry_time));

            launched_projectiles.push(AttackCruiserProjectileSpawn {
                projectile_id,
                origin: launch_location.origin,
                speed: launch_location.speed,
                yaw: launch_location.yaw,
                pitch: launch_location.pitch,
            });
        }

        Ok(launched_projectiles)
    }

    pub fn hits<'a>(
        &mut self,
        actors: impl IntoIterator<Item = &'a AttackCruiserActor>,
        now: Instant,
        delta: Duration,
    ) -> BTreeMap<i32, Vec<(i32, AttackCruiserProjectileInstance)>> {
        let mut projectile_closest: BTreeMap<i32, (i32, f32)> = BTreeMap::new();

        for actor in actors {
            let Some(ship_bvh) = &actor.bvh else {
                continue;
            };

            let delta_secs = delta.as_secs_f32();
            let ship_pos = Vec3::from(actor.pos);
            let ship_roll = actor.ship.max_roll.to_radians() * actor.turn_multiplier;
            let ship_velocity = actor.speed * actor.forward_multiplier;
            let ship_angular_velocity = actor.angular_speed * actor.turn_multiplier;

            let aabb = ship_bvh.aabb();
            let min_array: [f32; 3] = aabb.min.coords.into();
            let max_array: [f32; 3] = aabb.max.coords.into();

            let min_corner = Vec3::from(min_array);
            let max_corner = Vec3::from(max_array);
            let ship_radius = min_corner.distance(max_corner) * 0.5;

            let mut step_cache = BTreeMap::new();

            for (projectile_id, projectile) in &self.live_projectiles {
                let launched_by_self = projectile.launched_by_actor_id == actor.id;
                let can_target_faction = projectile
                    .config
                    .target_factions
                    .iter()
                    .any(|faction| actor.ship.self_factions.contains(faction));
                if launched_by_self || !can_target_faction {
                    continue;
                }

                let projectile_speed = projectile.config.speed;
                let projectile_len = projectile.config.length;

                let secs_since_launch = now
                    .saturating_duration_since(projectile.launch_time)
                    .saturating_sub(delta)
                    .as_secs_f32();

                let global_speed = Vec3::from(projectile.speed);
                let global_start = Vec3::from(projectile.origin) + global_speed * secs_since_launch;

                let half_projectile_length = projectile_len * 0.5;
                let max_projectile_travel = projectile_speed * delta_secs + half_projectile_length;
                let max_ship_travel = Vec3::from(ship_velocity).length() * delta_secs;
                let max_reach = max_projectile_travel + max_ship_travel + ship_radius;

                let dist_to_ship_sq = global_start.distance_squared(ship_pos);
                if dist_to_ship_sq > max_reach * max_reach {
                    continue;
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

                let global_direction = Vec3::from(projectile.direction);
                let hit = (0..(max_steps as usize)).any(|step_index| {
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
                    let local_direction = inv_rotation_end * global_direction;

                    let half_length_offset = local_direction * half_projectile_length;

                    let check_start = local_start - half_length_offset;
                    let check_end = local_end + half_length_offset;

                    !ship_bvh.has_line_of_sight(check_start.to_array(), check_end.to_array())
                });

                if hit {
                    let entry = projectile_closest
                        .entry(*projectile_id)
                        .or_insert((actor.id, dist_to_ship_sq));
                    if dist_to_ship_sq < entry.1 {
                        *entry = (actor.id, dist_to_ship_sq);
                    }
                }
            }
        }

        let mut results: BTreeMap<i32, Vec<(i32, AttackCruiserProjectileInstance)>> =
            BTreeMap::new();
        for (projectile_id, (actor_id, _)) in projectile_closest {
            let projectile = self.remove_unchecked(projectile_id);

            results
                .entry(projectile.launched_by_actor_id)
                .or_default()
                .push((projectile_id, projectile.clone()));

            results
                .entry(actor_id)
                .or_default()
                .push((projectile_id, projectile));
        }

        results
    }

    pub fn expire(&mut self, now: Instant) {
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

struct AttackCruiserActorTarget {
    id: i32,
    pos: Pos3,
    speed: Pos3,
    is_player: bool,
}

#[derive(Clone, Debug)]
struct AttackCruiserActorIdPool {
    last_id: i32,
}

impl AttackCruiserActorIdPool {
    const MAX_ACTOR_ID: i32 = i32::MAX;

    pub fn new() -> AttackCruiserActorIdPool {
        AttackCruiserActorIdPool { last_id: 0 }
    }

    pub fn next(
        &mut self,
        player_actor_ids: &[i32],
        npcs: &HashMap<i32, AttackCruiserActor>,
        ship_name: &String,
        actors_by_ship_name: &mut HashMap<String, HashSet<i32>>,
        max_alive: u16,
    ) -> Result<i32, ProcessPacketError> {
        let ids_with_ship_name = actors_by_ship_name.entry(ship_name.clone()).or_default();
        if ids_with_ship_name.len() as u16 >= max_alive {
            return Err(ProcessPacketError::new_with_log_level(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Attack Cruiser reached maximum number of actors with ship name {ship_name}"
                ),
                LogLevel::Debug,
            ));
        }

        let mut next_id = (self.last_id + 1) % Self::MAX_ACTOR_ID;
        while next_id == 0 || player_actor_ids.contains(&next_id) || npcs.contains_key(&next_id) {
            if next_id == self.last_id {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    "Attack Cruiser reached maximum number of actors".to_string(),
                ));
            }

            next_id = (next_id + 1) % Self::MAX_ACTOR_ID;
        }

        self.last_id = next_id;

        ids_with_ship_name.insert(next_id);
        Ok(next_id)
    }
}

#[derive(Clone, Debug)]
struct AttackCruiserPendingAoe {
    config: Arc<AttackCruiserAoeConfig>,
    launched_by_actor_id: i32,
    pos: Pos3,
}

#[derive(Clone, Debug)]
pub struct AttackCruiserGame {
    config: Arc<AttackCruiserConfig>,
    bvhs: HashMap<String, Arc<Bvh>>,
    player1: u32,
    player2: Option<u32>,
    player_states: ArrayVec<AttackCruiserPlayer, 2>,
    state: AttackCruiserGameState,
    active_players: ArrayVec<u32, 2>,
    active_player_indices: ArrayVec<u8, 2>,
    group: MinigameMatchmakingGroup,
    projectiles: AttackCruiserProjectilePool,
    npcs: HashMap<i32, AttackCruiserActor>,
    actors_by_ship_name: HashMap<String, HashSet<i32>>,
    actor_id_pool: AttackCruiserActorIdPool,
    pending_aoes: Vec<AttackCruiserPendingAoe>,
}

impl AttackCruiserGame {
    pub fn new(
        config: Arc<AttackCruiserConfig>,
        player1: u32,
        player2: Option<u32>,
        group: MinigameMatchmakingGroup,
        bvhs: &HashMap<String, Arc<Bvh>>,
    ) -> Self {
        config.validate_ships();
        let player_ship = config.ship(&config.player.ship);

        let bvhs = config.validate_bvhs(bvhs);
        let player_bvh = bvhs.get(&config.player.ship).cloned();

        let mut actor_id_pool = AttackCruiserActorIdPool::new();
        let mut npcs = HashMap::new();
        let mut actors_by_ship_name = HashMap::new();

        let now = Instant::now();

        let mut players = ArrayVec::new();
        players.push(player1);
        let mut player_states = ArrayVec::new();
        player_states.push(AttackCruiserPlayer::new(
            player1,
            actor_id_pool
                .next(
                    &[],
                    &npcs,
                    &config.player.ship,
                    &mut actors_by_ship_name,
                    player_ship.max_alive,
                )
                .expect("Attack Cruiser couldn't obtain player actor ID at startup"),
            player_ship.clone(),
            config.player.lives,
            config.player.max_lives,
            config.player.spawn1.pos,
            config.player.spawn1.yaw.to_radians(),
            player_bvh.clone(),
            now,
        ));

        if let Some(player2) = player2 {
            players.push(player2);
            player_states.push(AttackCruiserPlayer::new(
                player2,
                actor_id_pool
                    .next(
                        &[player_states[0].actor.id],
                        &npcs,
                        &config.player.ship,
                        &mut actors_by_ship_name,
                        player_ship.max_alive,
                    )
                    .expect("Attack Cruiser couldn't obtain player actor ID at startup"),
                player_ship,
                config.player.lives,
                config.player.max_lives,
                config.player.spawn2.pos,
                config.player.spawn2.yaw.to_radians(),
                player_bvh.clone(),
                now,
            ));
        }

        // TODO: remove test NPC
        let player_actor_ids = &player_states
            .iter()
            .map(|player_state| player_state.actor.id)
            .collect::<Vec<i32>>();
        let test_npc_ship_name = "test".to_string();
        let test_npc_ship = config.ship(&test_npc_ship_name);
        let npc_id1 = actor_id_pool
            .next(
                player_actor_ids,
                &npcs,
                &test_npc_ship_name,
                &mut actors_by_ship_name,
                test_npc_ship.max_alive,
            )
            .unwrap();
        npcs.insert(
            npc_id1,
            AttackCruiserActor::new(
                npc_id1,
                config.player.spawn2.pos
                    + Pos3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                config.player.spawn2.yaw.to_radians(),
                test_npc_ship.max_speed,
                0.0,
                player_bvh.clone(),
                test_npc_ship.clone(),
                now,
            ),
        );
        let npc_id2 = actor_id_pool
            .next(
                player_actor_ids,
                &npcs,
                &test_npc_ship_name,
                &mut actors_by_ship_name,
                test_npc_ship.max_alive,
            )
            .unwrap();
        npcs.insert(
            npc_id2,
            AttackCruiserActor::new(
                npc_id2,
                config.player.spawn2.pos
                    + Pos3 {
                        x: 100.0,
                        y: 25.0,
                        z: 100.0,
                    },
                config.player.spawn2.yaw.to_radians(),
                test_npc_ship.max_speed,
                0.0,
                player_bvh.clone(),
                test_npc_ship.clone(),
                now,
            ),
        );
        let npc_id3 = actor_id_pool
            .next(
                player_actor_ids,
                &npcs,
                &test_npc_ship_name,
                &mut actors_by_ship_name,
                test_npc_ship.max_alive,
            )
            .unwrap();
        npcs.insert(
            npc_id3,
            AttackCruiserActor::new(
                npc_id3,
                config.player.spawn2.pos
                    - Pos3 {
                        x: 100.0,
                        y: 25.0,
                        z: 100.0,
                    },
                config.player.spawn2.yaw.to_radians(),
                test_npc_ship.max_speed,
                0.0,
                player_bvh,
                test_npc_ship.clone(),
                now,
            ),
        );

        AttackCruiserGame {
            bvhs,
            player1,
            player2,
            player_states,
            state: AttackCruiserGameState::WaitingForPlayersReady,
            actors_by_ship_name: HashMap::from([
                (
                    config.player.ship.clone(),
                    HashSet::from_iter(player_actor_ids.iter().copied()),
                ),
                (
                    test_npc_ship_name,
                    HashSet::from([npc_id1, npc_id2, npc_id3]),
                ),
            ]),
            active_player_indices: (0..players.len() as u8).collect(),
            active_players: players,
            group,
            config,
            projectiles: AttackCruiserProjectilePool::new(),
            npcs,
            actor_id_pool,
            pending_aoes: Vec::new(),
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
                    "global_config".to_string(),
                    AttackCruiserStartupConfigDefinition::Global(Box::new(
                        AttackCruiserGlobalConfig {
                            physics_speed: 1.0,
                            connect_timeout_seconds: 0.0,
                            ready_timeout_seconds: 0.0,
                            default_timeout_seconds: 0.0,
                            effects_preload_timeout_seconds: 60.0,
                            effects_ready_timeout_seconds: 60.0,
                            server_update_players_interval_seconds: 0.0,
                            server_update_actors_interval_seconds: 0.0,
                            server_draw_debug_data_interval_seconds: 0.0,
                            client_update_actors_interval_seconds: f32::from(
                                self.config.client_actor_update_interval_millis,
                            ) / 1000.0,
                            // The client uses a fixed 10% interpolation steps for rotation,
                            // so use 10% interpolation steps for position for consistency
                            max_interpolation_step: 0.1,
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
                    "".to_string(),
                    AttackCruiserStartupConfigDefinition::Game(Box::new(AttackCruiserGameConfig {
                        id: self.group.stage_guid,
                        encounter_id: 0,
                        sound_id: self.config.sound_id,
                        challenge_mode: (&self.config.challenge).into(),
                        global_config: AttackCruiserStartupConfigReference {
                            class: AttackCruiserStartupConfigClass::Global,
                            name: "global_config".to_string(),
                        },
                        end_condition_config: AttackCruiserStartupConfigReference {
                            class: AttackCruiserStartupConfigClass::Condition,
                            name: "".to_string(),
                        },
                        win_condition_config: AttackCruiserStartupConfigReference {
                            class: AttackCruiserStartupConfigClass::Condition,
                            name: "".to_string(),
                        },
                        target_value: self.config.challenge.value(),
                        target_value2: 0,
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
                        planet: AttackCruiserPlanetStartupConfig {
                            model_id: self.config.planet.model_id,
                            pos: self.config.planet.center,
                            angular_speed: self.config.planet.angular_speed.to_radians(),
                        },
                        players: AttackCruiserVec::new(),
                        events: AttackCruiserVec(
                            "".to_string(),
                            vec![AttackCruiserEventConfig {
                                event_type: AttackCruiserEventType::Intro,
                                cinematics: AttackCruiserVec(
                                    "".to_string(),
                                    self.config
                                        .intro
                                        .cinematics
                                        .iter()
                                        .map(|cinematic| cinematic.into())
                                        .collect(),
                                ),
                                event_actors: AttackCruiserVec(
                                    "".to_string(),
                                    self.config
                                        .intro
                                        .ships
                                        .iter()
                                        .map(|actor| actor.into())
                                        .collect(),
                                ),
                            }],
                        ),
                        actor_pools: AttackCruiserVec(
                            "".to_string(),
                            self.config
                                .ships
                                .iter()
                                .map(|(name, ship)| AttackCruiserActorPoolConfig {
                                    actor_config: AttackCruiserStartupConfigReference {
                                        class: AttackCruiserStartupConfigClass::Ship,
                                        name: ship_startup_config_name(name),
                                    },
                                    size: ship.max_alive.into(),
                                })
                                .collect(),
                        ),
                        waves: AttackCruiserVec::new(),
                    })),
                ),
                camera_config: AttackCruiserStartupConfig::new(
                    "".to_string(),
                    AttackCruiserStartupConfigDefinition::Camera(Box::new(
                        AttackCruiserStartupCameraConfig {
                            default_distance: self.config.camera.default_distance,
                            min_distance: self.config.camera.min_distance,
                            max_distance: self.config.camera.max_distance,
                            pitch_degrees: self.config.camera.pitch.to_degrees(),
                            min_pitch_degrees: self.config.camera.pitch.to_degrees(),
                            max_pitch_degrees: self.config.camera.pitch.to_degrees(),
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
                            forward_tether: AttackCruiserBool(false),
                            forward_tether_seconds: 0.0,
                            near_clip_distance: self.config.camera.near_clip_distance,
                            particle_update_distance: self.config.actor_update_radius,
                            actor_update_radius: self.config.actor_update_radius,
                            shadow_quality: 3,
                            shadow_draw_distance: self.config.actor_update_radius,
                            shadow_blob_render_distance: self.config.actor_update_radius,
                            overhead_render_distance: self.config.actor_update_radius,
                        },
                    )),
                ),
                configs: self
                    .config
                    .ships
                    .iter()
                    .flat_map(|(name, ship)| {
                        iter::once(AttackCruiserStartupConfig::new(
                            ship_physics_startup_config_name(name),
                            AttackCruiserStartupConfigDefinition::ComplexPhysics(Box::new(
                                AttackCruiserComplexPhysicsConfig {
                                    base_config: AttackCruiserBasePhysicsConfig {
                                        contact_response: AttackCruiserBool(true),
                                        mass: match ship.show_arrow_to_player {
                                            true => 1.0,
                                            false => 0.1,
                                        },
                                        length: 1.0,
                                        width: 1.0,
                                        height: 1.0,
                                        center_of_mass_z: 0.0,
                                        max_speed: ship.max_speed,
                                        vertical_speed: 0.0,
                                    },
                                    reverse_speed: -ship.max_speed,
                                    turbo_speed: 0.0,
                                    stationary_turn: ship.stationary_turn,
                                    gears: AttackCruiserVec(
                                        "".to_string(),
                                        vec![AttackCruiserComplexPhysicsGear {
                                            shift_up_speed: 0.0,
                                            shift_down_speed: 0.0,
                                            base_acceleration: ship.acceleration,
                                            base_deceleration: ship.deceleration,
                                            turbo_acceleration: 0.0,
                                            brake_deceleration: 0.0,
                                            sideways_deceleration: 0.0,
                                            angular_acceleration: ship
                                                .angular_acceleration
                                                .to_radians(),
                                            turbo_angular_acceleration: 0.0,
                                            angular_deceleration: ship
                                                .angular_deceleration
                                                .to_radians(),
                                            max_angular_speed: ship.max_angular_speed.to_radians(),
                                            turbo_max_angular_speed: 0.0,
                                        }],
                                    ),
                                },
                            )),
                        ))
                        .chain(iter::once(
                            AttackCruiserStartupConfig::new(
                                ship_startup_config_name(name),
                                AttackCruiserStartupConfigDefinition::Ship(Box::new(
                                    AttackCruiserShipStartupConfig {
                                        actor_config: AttackCruiserActorConfig {
                                            model_id: ship.model_id,
                                            effect_id: 0,
                                            death_effect_id: 0,
                                            despawn_effect_id: 0,
                                            explode_offset: 0.0,
                                            collision_asset_name: ship
                                                .asset_name
                                                .as_ref()
                                                .map(|asset_name| format!("{asset_name}.cdt",))
                                                .unwrap_or_default(),
                                            physics_config: AttackCruiserStartupConfigReference {
                                                class:
                                                    AttackCruiserStartupConfigClass::ComplexPhysics,
                                                name: ship_physics_startup_config_name(name),
                                            },
                                            max_health: ship.max_health.into(),
                                            explosive_collision: AttackCruiserBool(false),
                                            collision_damage: 0,
                                            score: 0,
                                            bonus_score: 0,
                                            bonus_max_age_seconds: 0.0,
                                            overhead_offset_y: 0.0,
                                            overhead_health_scale: ship.overhead_health_scale,
                                            animations: AttackCruiserVec(
                                                "".to_string(),
                                                ship.animations
                                                    .iter()
                                                    .map(|animation| animation.into())
                                                    .collect(),
                                            ),
                                            cinematics: AttackCruiserVec(
                                                "".to_string(),
                                                ship.cinematics
                                                    .iter()
                                                    .map(|cinematic| cinematic.into())
                                                    .collect(),
                                            ),
                                            damage_states: AttackCruiserVec(
                                                "".to_string(),
                                                ship.damage_states
                                                    .iter()
                                                    .map(|damage_state| damage_state.into())
                                                    .collect(),
                                            ),
                                        },
                                        thruster_effect_id: ship
                                            .thruster_effect_id
                                            .unwrap_or_default(),
                                        invulnerable_effect_id: ship
                                            .invulnerable_effect_id
                                            .unwrap_or_default(),
                                        stunned_effect_id: ship
                                            .stunned_effect_id
                                            .unwrap_or_default(),
                                        weapons: AttackCruiserVec::new(),
                                        roll_max_angle: ship.max_roll.to_degrees(),
                                        pitch_max_angle: 0.0,
                                        continuous_fire_seconds: 0.05,
                                        fire_cooldown_seconds: f32::from(
                                            ship.weapons.cooldown_millis(),
                                        ) / 1000.0,
                                    },
                                )),
                            ),
                        ))
                    })
                    .collect(),
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
        if !matches!(self.state, AttackCruiserGameState::WaveActive) {
            return Vec::new();
        }

        let mut broadcasts = Vec::new();
        let mut pending_npcs = Vec::new();

        let actor_iter = self
            .active_player_indices
            .iter()
            .copied()
            .filter(|player_index| {
                !self.player_states[*player_index as usize].dead()
                    && !self.player_states[*player_index as usize].warped_away()
            })
            .map(|player_index| &self.player_states[player_index as usize].actor)
            .chain(self.npcs.values().filter(|npc| !npc.dead()));

        let hits = self
            .projectiles
            .hits(actor_iter.clone(), now, tick_duration);
        self.projectiles.expire(now);

        let mut aoe_effect_packets = Vec::new();
        let mut aoes: HashMap<i32, Vec<(i32, Arc<AttackCruiserAoeConfig>)>> = HashMap::new();
        for aoe in self.pending_aoes.drain(..) {
            aoe_effect_packets.append(&mut Self::spawn_client_effect(
                aoe.config.composite_effect_id,
                aoe.pos,
                self.group,
            ));

            actor_iter
                .clone()
                .filter(|target| {
                    distance3_sq(
                        target.pos.x,
                        target.pos.y,
                        target.pos.z,
                        aoe.pos.x,
                        aoe.pos.y,
                        aoe.pos.z,
                    ) <= aoe.config.radius * aoe.config.radius
                })
                .for_each(|target| {
                    let can_target_faction = aoe
                        .config
                        .target_factions
                        .iter()
                        .any(|faction| target.ship.self_factions.contains(faction));

                    if !can_target_faction {
                        return;
                    }

                    aoes.entry(target.id)
                        .or_default()
                        .push((aoe.launched_by_actor_id, aoe.config.clone()))
                })
        }
        broadcasts.push(Broadcast::Multi(
            self.active_players.to_vec(),
            aoe_effect_packets,
        ));

        let actors_by_faction = Self::list_actors_by_faction(
            &self.active_player_indices,
            &self.player_states,
            &self.npcs,
            AttackCruiserPlayer::seekable,
            AttackCruiserActor::seekable,
        );
        self.tick_players(now, &mut broadcasts, &hits, &aoes, &mut pending_npcs);
        self.tick_npcs(
            now,
            tick_duration,
            &mut broadcasts,
            &hits,
            &aoes,
            &mut pending_npcs,
            &actors_by_faction,
        );

        let unique_hits: HashMap<i32, AttackCruiserProjectileInstance> = hits
            .into_values()
            .flat_map(|hits| hits.into_iter())
            .collect();

        broadcasts.push(Broadcast::Multi(
            self.active_players.to_vec(),
            unique_hits
                .into_iter()
                .filter_map(|(projectile_id, projectile)| {
                    projectile.config.composite_effect_id?;

                    Some(GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: AttackCruiserRemoveProjectile {
                            minigame_header: MinigameHeader {
                                stage_guid: self.group.stage_guid,
                                sub_op_code: AttackCruiserOpCode::RemoveProjectile as i32,
                                stage_group_guid: self.group.stage_group_guid,
                            },
                            projectile_id,
                            despawn_effect_id: projectile.config.hit_composite_effect_id,
                            delay_seconds: 0.0,
                        },
                    }))
                })
                .collect(),
        ));

        broadcasts.append(&mut self.finalize_actors(pending_npcs));

        broadcasts.push(Broadcast::Multi(
            self.active_players.to_vec(),
            self.update_client_players_once_ready(AttackCruiserPlayerStateType {
                index: false,
                score: true,
                unknown3: false,
                inventory: true,
                actor_id: false,
            }),
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
            player_state.pause_or_resume(pause);
        });
        self.npcs.values_mut().for_each(|npc| {
            npc.pause_or_resume(pause);
        });
        Ok(Vec::new())
    }

    pub fn remove_player(
        &mut self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<MinigameRemovePlayerResult, ProcessPacketError> {
        let player_index = self.player_index(player)? as usize;

        let mut packets = Self::despawn_client_actor(
            &self.player_states[player_index].actor,
            self.player_states[player_index]
                .actor
                .ship
                .despawn_effect_id,
            &mut self.actors_by_ship_name,
            self.group,
        );
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
        let broadcasts = vec![Broadcast::Multi(self.active_players.to_vec(), packets)];

        self.active_players
            .retain(|active_player| *active_player != player);
        self.active_player_indices
            .retain(|player_index| self.player_states[*player_index as usize].guid != player);
        let player_state = &mut self.player_states[player_index];
        player_state.set_lives_unchecked(0);

        minigame_status.total_score = player_state.score;
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

        broadcasts.push(self.update_server_player_actor(player_index));

        Ok(broadcasts)
    }

    pub fn handle_click(
        &mut self,
        sender: u32,
        click: AttackCruiserClickedLocation,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = self.player_index(sender)?;
        let player_state = &self.player_states[player_index as usize];

        let now = Instant::now();
        if player_state.disarmed() {
            return Ok(Vec::new());
        }

        if matches!(click.click_type, AttackCruiserClickType::Right) {
            let player_state = &mut self.player_states[player_index as usize];
            let secondary_item_opt =
                player_state
                    .use_secondary_item()
                    .and_then(|secondary_item_name| {
                        self.config.player.secondary_items.get(secondary_item_name)
                    });
            let mut pending_npcs = Vec::new();

            if let Some(secondary_item) = secondary_item_opt {
                if secondary_item.invulnerability_millis > 0 {
                    player_state.use_invulnerable_powerup(
                        Duration::from_millis(secondary_item.invulnerability_millis.into()),
                        now,
                    );
                }

                pending_npcs.append(&mut Self::launch_actors_from_actor(
                    &player_state.actor,
                    &secondary_item.ships,
                    now,
                    &self.config,
                    &self.bvhs,
                ));

                if let Some(config) = &secondary_item.aoe {
                    self.pending_aoes.push(AttackCruiserPendingAoe {
                        config: config.clone(),
                        launched_by_actor_id: player_state.actor.id,
                        pos: player_state.actor.pos,
                    });
                }
            }
            return Ok(self.finalize_actors(pending_npcs));
        }

        let attacker_pos = Pos {
            x: player_state.actor.pos.x,
            y: player_state.actor.pos.y,
            z: player_state.actor.pos.z,
            w: 0.0,
        };
        let approx_target_pos = Pos {
            x: click.clicked_pos.x,
            y: player_state.actor.pos.y,
            z: click.clicked_pos.y,
            w: 0.0,
        };
        let approx_horizontal_distance_to_target = distance3_pos(attacker_pos, approx_target_pos);

        let actors_by_faction = Self::list_actors_by_faction(
            &self.active_player_indices,
            &self.player_states,
            &self.npcs,
            |player| !player.dead(),
            |npc| !npc.dead(),
        );
        let (target_y, horizontal_distance_to_target) = Self::closest_target(
            player_state.actor.id,
            &player_state.actor.ship.seek_factions,
            approx_target_pos.into(),
            &actors_by_faction,
            &self.config.playfield,
        )
        .map(|target| {
            (
                target.pos.y,
                distance3_pos(
                    attacker_pos,
                    Pos {
                        x: target.pos.x,
                        y: player_state.actor.pos.y,
                        z: target.pos.z,
                        w: 0.0,
                    },
                ),
            )
        })
        .unwrap_or((
            player_state.actor.pos.y,
            approx_horizontal_distance_to_target,
        ));
        let relative_y = target_y - player_state.actor.pos.y;

        let y_scale_factor =
            zero_nan(approx_horizontal_distance_to_target / horizontal_distance_to_target);

        let target_pos = Pos3 {
            x: click.clicked_pos.x,
            y: player_state.actor.pos.y + relative_y * y_scale_factor,
            z: click.clicked_pos.y,
        };

        let player_state = &mut self.player_states[player_index as usize];
        let (mut broadcasts, pending_npcs) = Self::actor_attack_primary(
            &mut player_state.actor,
            target_pos,
            Pos3::default(),
            now,
            &mut self.projectiles,
            &self.config,
            &self.bvhs,
            &self.active_players,
            self.group,
            self.config.max_weapon_cooldown_error_millis,
        )?;

        broadcasts.append(&mut self.finalize_actors(pending_npcs));

        Ok(broadcasts)
    }

    fn spawn_client_actor(&self, actor: &AttackCruiserActor, ship_config: &String) -> Vec<Vec<u8>> {
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserAddActor {
                minigame_header: MinigameHeader {
                    stage_guid: self.group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::AddActor as i32,
                    stage_group_guid: self.group.stage_group_guid,
                },
                actor_id: actor.id,
                hostility: actor.hostility(&self.config.factions),
                actor_config: AttackCruiserStartupConfigHash {
                    name: ship_startup_config_name(ship_config),
                    class: AttackCruiserStartupConfigClass::Ship,
                },
                pos: actor.pos,
                speed: actor.speed,
                yaw: actor.yaw,
                unknown7: 0,
            },
        })]
    }

    fn spawn_client_player_actor(&self, player_state: &AttackCruiserPlayer) -> Vec<Vec<u8>> {
        self.spawn_client_actor(&player_state.actor, &self.config.player.ship)
    }

    fn spawn_client_npc_actor(
        &self,
        actor: &AttackCruiserActor,
        ship_name: &String,
    ) -> Vec<Vec<u8>> {
        let mut packets = self.spawn_client_actor(actor, ship_name);

        packets.append(&mut self.set_actor_frozen(actor, None, false));

        packets
    }

    fn spawn_client_effect(
        effect_id: Option<u32>,
        pos: Pos3,
        group: MinigameMatchmakingGroup,
    ) -> Vec<Vec<u8>> {
        effect_id
            .map(|effect_id| {
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: AttackCruiserCompositeEffect {
                        minigame_header: MinigameHeader {
                            stage_guid: group.stage_guid,
                            sub_op_code: AttackCruiserOpCode::CompositeEffect as i32,
                            stage_group_guid: group.stage_group_guid,
                        },
                        effect_id,
                        pos,
                    },
                })]
            })
            .unwrap_or_default()
    }

    fn despawn_client_actor(
        actor: &AttackCruiserActor,
        despawn_effect_id: Option<u32>,
        actors_by_ship_name: &mut HashMap<String, HashSet<i32>>,
        group: MinigameMatchmakingGroup,
    ) -> Vec<Vec<u8>> {
        // There are usually very few ship types (<10), so the performance cost of iterating
        // over all ship types is low. If we kept a map of strings to usize, we would need
        // to clone a string or reference to a string for every actor. That would require
        // more memory than storing extra i32s and may induce more CPU cache misses than
        // a contiguous array of IDs.
        actors_by_ship_name.retain(|_, ids| {
            ids.remove(&actor.id);
            !ids.is_empty()
        });

        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserRemoveActor {
                minigame_header: MinigameHeader {
                    stage_guid: group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::RemoveActor as i32,
                    stage_group_guid: group.stage_group_guid,
                },
                actor_id: actor.id,
            },
        })];

        packets.append(&mut Self::spawn_client_effect(
            despawn_effect_id,
            actor.pos,
            group,
        ));

        packets
    }

    fn replace_client_player_actor(&mut self, player_index: u8) -> Vec<Vec<u8>> {
        let player_actor_ids = &self.player_actor_ids();
        let mut packets = Self::despawn_client_actor(
            &self.player_states[player_index as usize].actor,
            self.player_states[player_index as usize]
                .actor
                .ship
                .death_end_effect_id,
            &mut self.actors_by_ship_name,
            self.group,
        );
        let player_state = &mut self.player_states[player_index as usize];
        // Reusing the actor ID tends to break animations, but at least the game remains playable
        player_state.actor.id = self
            .actor_id_pool
            .next(
                player_actor_ids,
                &self.npcs,
                &self.config.player.ship,
                &mut self.actors_by_ship_name,
                player_state.actor.ship.max_alive,
            )
            .unwrap_or(player_state.actor.id);
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
                guid: player_guid(self.player_states[player_index as usize].guid),
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
                states: self
                    .active_player_indices
                    .iter()
                    .map(|&player_index| AttackCruiserPlayerUpdate {
                        player_index: player_index.into(),
                        state: self.player_state_update(player_index, update_type),
                    })
                    .collect(),
            },
        })]
    }

    fn update_server_player_actor(&self, player_index: u8) -> Broadcast {
        let player_state = &self.player_states[player_index as usize];
        let warp_out = !player_state.dead()
            && player_state
                .bounds
                .has_phase(AttackCruiserPlayerBoundsPhase::Outside);

        Broadcast::Multi(
            self.active_players.to_vec(),
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
                            show_invulnerablity_effect: player_state.used_invulnerable_powerup(),
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
                            show_stun_effect: player_state.stunned(),
                            hide_ring: warp_out,
                            show_boss_ring: false,
                        },
                    }],
                },
            })],
        )
    }

    fn update_server_npc_actor(
        actor: &AttackCruiserActor,
        warp_out: bool,
        group: MinigameMatchmakingGroup,
    ) -> Vec<u8> {
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AttackCruiserUpdateServerActors {
                minigame_header: MinigameHeader {
                    stage_guid: group.stage_guid,
                    sub_op_code: AttackCruiserOpCode::UpdateActors as i32,
                    stage_group_guid: group.stage_group_guid,
                },
                states: vec![AttackCruiserActorUpdate {
                    actor_id: actor.id,
                    pos: actor.pos,
                    yaw: actor.yaw,
                    speed: actor.speed,
                    angular_speed: actor.angular_speed,
                    forward_multiplier: actor.forward_multiplier,
                    turn_multiplier: actor.turn_multiplier,
                    health: actor.health.into(),
                    state: AttackCruiserActorState {
                        unknown1: false,
                        unknown2: false,
                        show_invulnerablity_effect: actor.used_invulnerable_powerup(),
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
                        show_stun_effect: actor.stunned(),
                        hide_ring: warp_out,
                        show_boss_ring: false,
                    },
                }],
            },
        })
    }

    fn set_actor_frozen(
        &self,
        actor: &AttackCruiserActor,
        guid_if_player: Option<u32>,
        frozen: bool,
    ) -> Vec<Vec<u8>> {
        let guid = player_guid(guid_if_player.unwrap_or_default());
        vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: AttackCruiserQueueCommand {
                    minigame_header: MinigameHeader {
                        stage_guid: self.group.stage_guid,
                        sub_op_code: AttackCruiserOpCode::QueueCommand as i32,
                        stage_group_guid: self.group.stage_group_guid,
                    },
                    actor_id: actor.id,
                    command: AttackCruiserCommand::Movable(AttackCruiserBoolCommand {
                        guid,
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
                    actor_id: actor.id,
                    command: AttackCruiserCommand::Collision(AttackCruiserBoolCommand {
                        guid,
                        value: !frozen && actor.ship.enable_collision,
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

        for player_index in self.active_player_indices.iter().copied() {
            let player_state = &self.player_states[player_index as usize];
            packets.append(&mut self.set_actor_frozen(
                &player_state.actor,
                Some(player_state.guid),
                false,
            ));
        }

        // TODO: remove and spawn in waves
        self.npcs.values().for_each(|npc| {
            packets.append(&mut self.spawn_client_npc_actor(npc, &String::from("test")));
        });

        Ok(vec![Broadcast::Multi(
            self.active_players.to_vec(),
            packets,
        )])
    }

    fn actor_attack_primary<'a>(
        actor: &'a mut AttackCruiserActor,
        target_pos: Pos3,
        target_speed: Pos3,
        now: Instant,
        projectile_pool: &mut AttackCruiserProjectilePool,
        config: &'a AttackCruiserConfig,
        bvhs: &HashMap<String, Arc<Bvh>>,
        active_players: &[u32],
        group: MinigameMatchmakingGroup,
        max_cooldown_error_millis: u16,
    ) -> Result<(Vec<Broadcast>, Vec<AttackCruiserPendingActor>), ProcessPacketError> {
        let mut packets = Vec::new();
        let rng = &mut thread_rng();

        let actor_id = actor.id;
        let actor_pos = actor.pos;
        let (projectiles, actors) = actor.attack_primary(
            now,
            Duration::from_millis(max_cooldown_error_millis.into()),
            target_pos,
            target_speed,
        );
        for (projectile, direction) in projectiles {
            packets.extend(
                projectile_pool
                    .launch(rng, actor_id, actor_pos, direction, projectile, now)?
                    .into_iter()
                    .filter_map(|launched_projectile| {
                        Some(GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: AttackCruiserAddProjectile {
                                minigame_header: MinigameHeader {
                                    stage_guid: group.stage_guid,
                                    sub_op_code: AttackCruiserOpCode::AddProjectile as i32,
                                    stage_group_guid: group.stage_group_guid,
                                },
                                projectile_id: launched_projectile.projectile_id,
                                unknown2: 0,
                                effect_id: projectile.composite_effect_id?,
                                despawn_effect_id: 0,
                                lifetime_seconds: f32::from(projectile.lifetime_millis) / 1000.0,
                                origin: launched_projectile.origin,
                                speed: launched_projectile.speed,
                                unknown8: Pos3::default(),
                                yaw: launched_projectile.yaw,
                                pitch: launched_projectile.pitch,
                                unknown11: 0.0,
                                unknown12: 0.0,
                                unknown13: 0,
                            },
                        }))
                    }),
            );
        }

        let mut new_npcs = Vec::new();
        for (launched_actor, direction) in actors {
            let ship = config.ship(&launched_actor.ship);
            let bvh = bvhs.get(&launched_actor.ship).cloned();

            for _ in 0..launched_actor.count {
                let launch_vector = launch_vector(
                    rng,
                    actor_pos,
                    direction,
                    ship.max_speed,
                    launched_actor.wobble,
                    launched_actor.yaw,
                    launched_actor.launch_offset,
                    launched_actor.launch_height,
                );
                new_npcs.push(AttackCruiserPendingActor::new(
                    launch_vector.origin,
                    launch_vector.yaw,
                    ship.max_speed,
                    0.0,
                    bvh.clone(),
                    ship.clone(),
                    launched_actor.ship.clone(),
                    now,
                ));
            }
        }

        Ok((
            vec![Broadcast::Multi(active_players.to_vec(), packets)],
            new_npcs,
        ))
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

    fn player_actor_ids(&self) -> Vec<i32> {
        self.active_player_indices
            .iter()
            .map(|player_index| self.player_states[*player_index as usize].actor.id)
            .collect()
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
                    weapon_tier: 0,
                    primary_quantity: 0,
                    special_quantity: player_state.secondary_item_count.into(),
                    unknown4: 0,
                    special_icon_id: (player_state.secondary_item_count > 0)
                        .then(|| {
                            self.config
                                .player
                                .secondary_items
                                .get(&player_state.secondary_item)
                                .map(|item| item.icon_id)
                        })
                        .flatten()
                        .unwrap_or_default(),
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

    fn total_actor_deltas(
        hit_actor_id: i32,
        projectiles: &[(i32, AttackCruiserProjectileInstance)],
        aoes: &[(i32, Arc<AttackCruiserAoeConfig>)],
    ) -> AttackCruiserActorDeltas {
        projectiles
            .iter()
            .map(|(_, projectile)| {
                (
                    projectile.launched_by_actor_id,
                    &projectile.config.self_deltas,
                    &projectile.config.target_deltas,
                )
            })
            .chain(aoes.iter().map(|(launched_by_actor_id, aoe)| {
                (*launched_by_actor_id, &aoe.self_deltas, &aoe.target_deltas)
            }))
            .fold(
                AttackCruiserActorDeltas::default(),
                |total_deltas, (launched_by_actor_id, self_deltas, target_deltas)| {
                    let deltas = match hit_actor_id == launched_by_actor_id {
                        true => self_deltas,
                        false => target_deltas,
                    };

                    total_deltas.saturating_add(deltas)
                },
            )
    }

    fn tick_players(
        &mut self,
        now: Instant,
        broadcasts: &mut Vec<Broadcast>,
        hits: &BTreeMap<i32, Vec<(i32, AttackCruiserProjectileInstance)>>,
        aoes: &HashMap<i32, Vec<(i32, Arc<AttackCruiserAoeConfig>)>>,
        pending_npcs: &mut Vec<AttackCruiserPendingActor>,
    ) {
        for player_index in self.active_player_indices.clone().into_iter() {
            let player_index = player_index as usize;
            let player_state = &mut self.player_states[player_index];
            if player_state.paused() {
                continue;
            }

            let in_bounds = is_inside_oval(
                player_state.actor.pos,
                self.config.playfield.center,
                self.config.playfield.radius_x,
                self.config.playfield.radius_z,
            );

            let mut update_clients = match (&mut player_state.bounds.phase(), in_bounds) {
                (AttackCruiserPlayerBoundsPhase::Inside, true) => false,
                (AttackCruiserPlayerBoundsPhase::Inside, false) => {
                    self.player_states[player_index].bounds.set_phase(
                        AttackCruiserPlayerBoundsPhase::OutsideWaitingToWarp,
                        Duration::from_millis(
                            self.config.player.out_of_bounds_warp_delay_millis.into(),
                        ),
                        now,
                    );
                    true
                }
                (AttackCruiserPlayerBoundsPhase::Outside, _) => {
                    if self.player_states[player_index]
                        .bounds
                        .has_completed_phase(now)
                    {
                        self.player_states[player_index].bounds.set_phase(
                            AttackCruiserPlayerBoundsPhase::OutsideWaitingToWarp,
                            Duration::from_millis(
                                self.config.player.out_of_bounds_warp_delay_millis.into(),
                            ),
                            now,
                        );
                    }
                    true
                }
                (AttackCruiserPlayerBoundsPhase::OutsideWaitingToWarp, true) => {
                    self.player_states[player_index].bounds.set_in_bounds();
                    true
                }
                (AttackCruiserPlayerBoundsPhase::OutsideWaitingToWarp, false) => {
                    if self.player_states[player_index]
                        .bounds
                        .has_completed_phase(now)
                    {
                        self.player_states[player_index].bounds.set_phase(
                            AttackCruiserPlayerBoundsPhase::Outside,
                            Duration::from_millis(
                                self.config.player.out_of_bounds_warp_millis.into(),
                            ),
                            now,
                        );
                    }
                    true
                }
            };

            let player_state = &mut self.player_states[player_index];
            if player_state.completed_stun(now) {
                player_state.remove_stun();
            }

            if player_state.respawnable(now) {
                pending_npcs.append(&mut Self::launch_actors_from_actor(
                    &player_state.actor,
                    &player_state.actor.ship.ships_on_death,
                    now,
                    &self.config,
                    &self.bvhs,
                ));

                player_state.respawn(
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

                broadcasts.push(Broadcast::Multi(
                    self.active_players.to_vec(),
                    actor_packets,
                ));
            } else if player_state.lost(now) {
                broadcasts.push(Broadcast::Single(
                    self.player_states[player_index].guid,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: ExecuteScriptWithStringParams {
                            script_name: "StarDestroyerHandler.quitGame".to_string(),
                            params: vec![],
                        },
                    })],
                ));
            } else if player_state.completed_respawn(now) {
                player_state.complete_respawn();

                let player_state = &self.player_states[player_index];
                broadcasts.push(Broadcast::Multi(
                    self.active_players.to_vec(),
                    self.set_actor_frozen(&player_state.actor, Some(player_state.guid), false),
                ));
            } else if player_state.completed_invulnerable_powerup(now) {
                player_state.complete_invulnerable_powerup();
            }

            let player_state = &mut self.player_states[player_index];
            if !player_state.dead() {
                let actor = &mut player_state.actor;
                let total_deltas = Self::total_actor_deltas(
                    actor.id,
                    hits.get(&actor.id).map(|v| v.as_slice()).unwrap_or(&[]),
                    aoes.get(&actor.id).map(|v| v.as_slice()).unwrap_or(&[]),
                );
                player_state.add_health(total_deltas.health, now);
                player_state.add_primary_tiers(total_deltas.primary_tiers);
                player_state.add_lives(total_deltas.player_lives);
                player_state.stun(Duration::from_millis(total_deltas.stun_millis.into()), now);
                if let Some(secondary_item) = total_deltas.secondary_item {
                    player_state.add_secondary_item(&secondary_item.name, secondary_item.count);
                }

                if player_state.dead() {
                    let mut death_packets = Self::spawn_client_effect(
                        player_state.actor.ship.death_start_effect_id,
                        player_state.actor.pos,
                        self.group,
                    );

                    let player_state = &self.player_states[player_index];
                    death_packets.append(&mut self.set_actor_frozen(
                        &player_state.actor,
                        Some(player_state.guid),
                        true,
                    ));
                    death_packets.append(&mut self.update_client_players_once_ready(
                        AttackCruiserPlayerStateType {
                            index: false,
                            score: true,
                            unknown3: false,
                            inventory: false,
                            actor_id: false,
                        },
                    ));
                    broadcasts.push(Broadcast::Multi(
                        self.active_players.to_vec(),
                        death_packets,
                    ));

                    update_clients = true;
                }
            }

            let player_state = &mut self.player_states[player_index];
            let health_percent = zero_nan(
                player_state.actor.health as f32 / player_state.actor.ship.max_health as f32
                    * 100.0,
            );
            let is_low_health = health_percent <= self.config.player.damage_alarm_health_percent;
            let is_damage_alarm_timer_expired = player_state
                .damage_alarm_sound_timer
                .time_until_next_event(now)
                .is_zero();
            if is_low_health && !player_state.dead() && is_damage_alarm_timer_expired {
                broadcasts.push(Broadcast::Single(
                    player_state.guid,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: PlaySoundIdOnTarget {
                            sound_id: self.config.player.damage_alarm_sound_id,
                            target: Target::None,
                        },
                    })],
                ));
                player_state.damage_alarm_sound_timer.schedule_event(
                    Duration::from_millis(self.config.player.damage_alarm_interval_millis.into()),
                    now,
                );
            }

            if update_clients {
                broadcasts.push(self.update_server_player_actor(player_index as u8));
            }
        }
    }

    fn tick_npcs(
        &mut self,
        now: Instant,
        tick_duration: Duration,
        broadcasts: &mut Vec<Broadcast>,
        hits: &BTreeMap<i32, Vec<(i32, AttackCruiserProjectileInstance)>>,
        aoes: &HashMap<i32, Vec<(i32, Arc<AttackCruiserAoeConfig>)>>,
        pending_npcs: &mut Vec<AttackCruiserPendingActor>,
        actors_by_faction: &HashMap<String, Vec<AttackCruiserActorTarget>>,
    ) {
        self.npcs.retain(|_, npc| {
            if npc.paused() {
                return true;
            }

            if npc.completed_invulnerable_powerup(now) {
                npc.set_vulnerable();
            }

            if npc.completed_stun(now) {
                npc.remove_stun();
            }

            let (is_real_target, target_pos, target_speed) = Self::closest_target(
                npc.id,
                &npc.ship.seek_factions,
                npc.pos,
                actors_by_faction,
                &self.config.playfield,
            )
            .map(|target| (true, target.pos, target.speed))
            .unwrap_or((false, self.config.playfield.center, Pos3::default()));
            npc.seek_target(target_pos, target_speed, tick_duration.as_secs_f32());

            if !npc.dead() {
                let total_deltas = Self::total_actor_deltas(
                    npc.id,
                    hits.get(&npc.id).map(|v| v.as_slice()).unwrap_or(&[]),
                    aoes.get(&npc.id).map(|v| v.as_slice()).unwrap_or(&[]),
                );
                npc.add_health(total_deltas.health, now);
                npc.add_primary_weapon_tiers(total_deltas.primary_tiers);
                npc.stun(Duration::from_millis(total_deltas.stun_millis.into()), now);

                if npc.dead() {
                    broadcasts.push(Broadcast::Multi(
                        self.active_players.to_vec(),
                        Self::spawn_client_effect(
                            npc.ship.death_start_effect_id,
                            npc.pos,
                            self.group,
                        ),
                    ));
                }
            }

            if is_real_target && !npc.disabled() {
                let attack_result = Self::actor_attack_primary(
                    npc,
                    target_pos,
                    target_speed,
                    now,
                    &mut self.projectiles,
                    &self.config,
                    &self.bvhs,
                    &self.active_players,
                    self.group,
                    self.config.max_weapon_cooldown_error_millis,
                );
                match attack_result {
                    Ok((mut attack_broadcasts, mut new_npcs)) => {
                        broadcasts.append(&mut attack_broadcasts);
                        pending_npcs.append(&mut new_npcs);
                    }
                    Err(err) => debug!("Attack Cruiser NPC was unable to attack: {}", err),
                }
            }

            broadcasts.push(Broadcast::Multi(
                self.active_players.to_vec(),
                vec![Self::update_server_npc_actor(npc, false, self.group)],
            ));

            if npc.completed_death(now) {
                pending_npcs.append(&mut Self::launch_actors_from_actor(
                    npc,
                    &npc.ship.ships_on_death,
                    now,
                    &self.config,
                    &self.bvhs,
                ));

                broadcasts.push(Broadcast::Multi(
                    self.active_players.to_vec(),
                    Self::despawn_client_actor(
                        npc,
                        npc.ship.death_end_effect_id,
                        &mut self.actors_by_ship_name,
                        self.group,
                    ),
                ));
                return false;
            }

            if npc.expired(now) {
                broadcasts.push(Broadcast::Multi(
                    self.active_players.to_vec(),
                    Self::despawn_client_actor(
                        npc,
                        npc.ship.despawn_effect_id,
                        &mut self.actors_by_ship_name,
                        self.group,
                    ),
                ));
                return false;
            }

            true
        });
    }

    fn list_actors_by_faction(
        active_player_indices: &[u8],
        player_states: &[AttackCruiserPlayer],
        npcs: &HashMap<i32, AttackCruiserActor>,
        player_filter: impl Fn(&AttackCruiserPlayer) -> bool,
        npc_filter: impl Fn(&AttackCruiserActor) -> bool,
    ) -> HashMap<String, Vec<AttackCruiserActorTarget>> {
        let mut actors_by_faction: HashMap<String, Vec<AttackCruiserActorTarget>> = HashMap::new();

        for player_index in active_player_indices.iter().copied() {
            let player_state = &player_states[player_index as usize];

            if player_filter(player_state) {
                let actor = &player_state.actor;
                for faction in actor.ship.self_factions.iter() {
                    let target = AttackCruiserActorTarget {
                        id: actor.id,
                        pos: actor.pos,
                        speed: actor.speed,
                        is_player: true,
                    };

                    if !actors_by_faction.contains_key(faction) {
                        actors_by_faction.insert(faction.to_owned(), vec![target]);
                        continue;
                    }

                    actors_by_faction
                        .get_mut(faction)
                        .expect("Actor faction list should have been initialized")
                        .push(target);
                }
            }
        }

        for npc in npcs.values().filter(|npc| npc_filter(npc)) {
            for faction in npc.ship.self_factions.iter() {
                let target = AttackCruiserActorTarget {
                    id: npc.id,
                    pos: npc.pos,
                    speed: npc.speed,
                    is_player: true,
                };

                if !actors_by_faction.contains_key(faction) {
                    actors_by_faction.insert(faction.to_owned(), vec![target]);
                    continue;
                }

                actors_by_faction
                    .get_mut(faction)
                    .expect("Actor faction list should have been initialized")
                    .push(target);
            }
        }

        actors_by_faction
    }

    fn launch_actors_from_actor(
        actor: &AttackCruiserActor,
        new_ships: &[AttackCruiserSpawnedShipConfig],
        now: Instant,
        config: &AttackCruiserConfig,
        bvhs: &HashMap<String, Arc<Bvh>>,
    ) -> Vec<AttackCruiserPendingActor> {
        let rng = &mut thread_rng();
        let mut new_npcs = Vec::new();

        let direction = Pos3 {
            x: actor.yaw.sin(),
            y: 0.0,
            z: actor.yaw.cos(),
        };

        for launched_actor in new_ships.iter() {
            let ship = config.ship(&launched_actor.ship);
            let bvh = bvhs.get(&launched_actor.ship).cloned();

            for _ in 0..launched_actor.count {
                let launch_vector = launch_vector(
                    rng,
                    actor.pos,
                    direction,
                    ship.max_speed,
                    launched_actor.wobble,
                    launched_actor.yaw,
                    launched_actor.launch_offset,
                    launched_actor.launch_height,
                );
                new_npcs.push(AttackCruiserPendingActor::new(
                    launch_vector.origin,
                    launch_vector.yaw,
                    ship.max_speed,
                    0.0,
                    bvh.clone(),
                    ship.clone(),
                    launched_actor.ship.clone(),
                    now,
                ));
            }
        }

        new_npcs
    }

    fn finalize_actors(&mut self, pending_npcs: Vec<AttackCruiserPendingActor>) -> Vec<Broadcast> {
        let player_actor_ids = &self.player_actor_ids();
        let mut new_npc_packets = Vec::new();
        for new_npc in pending_npcs.into_iter() {
            let id = match self.actor_id_pool.next(
                player_actor_ids,
                &self.npcs,
                new_npc.ship_name(),
                &mut self.actors_by_ship_name,
                new_npc.ship().max_alive,
            ) {
                Ok(id) => id,
                Err(err) => {
                    match err.log_level() {
                        LogLevel::Debug => debug!("Attack Cruiser couldn't spawn actor: {err}"),
                        LogLevel::Info => info!("Attack Cruiser couldn't spawn actor: {err}"),
                    }
                    continue;
                }
            };

            let (new_actor, ship_name) = new_npc.finalize(id);
            new_npc_packets.append(&mut self.spawn_client_npc_actor(&new_actor, &ship_name));
            self.npcs.insert(id, new_actor);
        }

        vec![Broadcast::Multi(
            self.active_players.to_vec(),
            new_npc_packets,
        )]
    }

    fn closest_target<'a>(
        actor_id: i32,
        seek_factions: &HashSet<String>,
        target_pos: Pos3,
        actors_by_faction: &'a HashMap<String, Vec<AttackCruiserActorTarget>>,
        playfield: &AttackCruiserPlayfieldConfig,
    ) -> Option<&'a AttackCruiserActorTarget> {
        let comparator = |target1: &&AttackCruiserActorTarget,
                          target2: &&AttackCruiserActorTarget| {
            let target1_in_bounds = is_inside_oval(
                target1.pos,
                playfield.center,
                playfield.radius_x,
                playfield.radius_z,
            );
            let target2_in_bounds = is_inside_oval(
                target2.pos,
                playfield.center,
                playfield.radius_x,
                playfield.radius_z,
            );

            if !target1_in_bounds && !target2_in_bounds {
                return Ordering::Equal;
            }

            if !target1_in_bounds {
                return Ordering::Greater;
            }

            if !target2_in_bounds {
                return Ordering::Less;
            }

            let distance1 = distance3_sq(
                target1.pos.x,
                0.0,
                target1.pos.z,
                target_pos.x,
                0.0,
                target_pos.z,
            );
            let distance2 = distance3_sq(
                target2.pos.x,
                0.0,
                target2.pos.z,
                target_pos.x,
                0.0,
                target_pos.z,
            );

            distance1.total_cmp(&distance2)
        };

        seek_factions
            .iter()
            .filter_map(|faction| actors_by_faction.get(faction))
            .flat_map(|targets| targets.iter())
            .filter(|target| target.id != actor_id)
            .min_by(comparator)
    }
}

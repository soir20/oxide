use num_enum::{IntoPrimitive, TryFromPrimitive};
use packet_serialize::{DeserializePacket, SerializePacket};

use crate::game_server::packets::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket, Pos, Pos3,
};

#[derive(
    Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket,
)]
#[repr(i32)]
pub enum AttackCruiserOpCode {
    ClientConfig = 0x1,
    UpdateGameState = 0x2,
    AddPlayer = 0x3,
    RemovePlayer = 0x4,
    ConfigPlayer = 0x5,
    RequestUpdatePlayers = 0x6,
    UpdatePlayers = 0x7,
    UpdateActors = 0x8,
    ClickOnLocation = 0xa,
    AddProjectile = 0xb,
    RemoveProjectile = 0xc,
    AddActor = 0xd,
    RemoveActor = 0xe,
    WorldEffect = 0xf,
    AddScore = 0x10,
    DebugRender = 0x11,
    DebugDrawData = 0x12,
    RoundTrip = 0x13,
    QueueCommand = 0x14,
    UpdateBossCount = 0x15,
}

pub struct AttackCruiserBool(pub bool);

impl SerializePacket for AttackCruiserBool {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.0.to_string().serialize(buffer);
    }
}

pub struct AttackCruiserVec<T>(pub String, pub Vec<T>);

impl<T> AttackCruiserVec<T> {
    pub fn new() -> Self {
        AttackCruiserVec("".to_string(), Vec::new())
    }
}

impl<T: SerializePacket> SerializePacket for AttackCruiserVec<T> {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        (self.1.len() as u32).serialize(buffer);
        for (index, entry) in self.1.iter().enumerate() {
            format!("{}[{index}]", self.0).serialize(buffer);
            entry.serialize(buffer);
        }
    }
}

fn hash_string(string: &str) -> u32 {
    let mut hash: i32 = 0;

    for &byte in string.as_bytes() {
        if byte == 0 {
            break;
        }

        let uppercase_byte = (byte as char).to_uppercase().next().unwrap_or(byte as char) as i32;
        let product = hash.wrapping_add(uppercase_byte).wrapping_mul(1025);
        hash = (product >> 6) ^ product;
    }

    let product = hash.wrapping_mul(9);
    (product ^ (product >> 11)).wrapping_mul(32769i32) as u32
}

#[derive(Clone, Copy, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket)]
#[repr(u32)]
pub enum AttackCruiserConfigClass {
    Actor = 0x16fcdb9,
    Ai = 0xe471290c,
    AiBehavior = 0x400f509e,
    BasePhysics = 0xe81c69d6,
    Blaster = 0x118d962f,
    BrainTimerAiBehavior = 0xd3c0f8cf,
    Camera = 0x6dc7e02b,
    ComplexPhysics = 0xa598eae0,
    Condition = 0x7d36f971,
    DeathSpawn = 0x9704178e,
    FollowTargetAiBehavior = 0x10f8260,
    Game = 0x4c61446a,
    Global = 0x79243a4c,
    GoToRandomTagAiBehavior = 0xa36fe7ee,
    GoToTagAiBehavior = 0x9e4d3c55,
    GoToTargetAiBehavior = 0xa10a5037,
    HealthPickup = 0x8dd15565,
    InventoryPickup = 0x62cf45ee,
    KillStreak = 0x6afb712e,
    LifePickup = 0xe787fee6,
    LifeTimeAiBehavior = 0xe6be56e3,
    MultiCondition = 0x103d673c,
    Path = 0x565115fe,
    Pickup = 0xc0843913,
    ReturnHomeAiBehavior = 0x88e7b12f,
    ScorePickup = 0xd0865beb,
    Ship = 0x4db6c82a,
    ShipBay = 0xebaf2508,
    SimplePhysics = 0x4b65ebe3,
    Squadron = 0xf27ff419,
    TagPosition = 0x84150070,
    TargetAiBehavior = 0xd12adb78,
    Torpedo = 0xb740760b,
    TorpedoBay = 0x729294d0,
    Wave = 0x23773492,
    WaveTimerAiBehavior = 0xf9d00c76,
    WaveVariableAiBehavior = 0x33b84b51,
    WeaponBay = 0x3e702d91,
    WeaponTierPickup = 0xab52827,
}

impl AttackCruiserConfigClass {
    pub fn name(&self) -> &'static str {
        match self {
            AttackCruiserConfigClass::Actor => "ActorConfig",
            AttackCruiserConfigClass::Ai => "AIConfig",
            AttackCruiserConfigClass::AiBehavior => "AIBehaviorConfig",
            AttackCruiserConfigClass::BasePhysics => "BasePhysicsConfig",
            AttackCruiserConfigClass::Blaster => "BlasterConfig",
            AttackCruiserConfigClass::BrainTimerAiBehavior => "SetBrainTimerAIBehaviorConfig",
            AttackCruiserConfigClass::Camera => "CameraConfig",
            AttackCruiserConfigClass::ComplexPhysics => "ComplexPhysicsConfig",
            AttackCruiserConfigClass::Condition => "ConditionConfig",
            AttackCruiserConfigClass::DeathSpawn => "DeathSpawnConfig",
            AttackCruiserConfigClass::FollowTargetAiBehavior => "FollowTargetAIBehaviorConfig",
            AttackCruiserConfigClass::Game => "GameConfig",
            AttackCruiserConfigClass::Global => "GlobalConfig",
            AttackCruiserConfigClass::GoToRandomTagAiBehavior => "GotoRandomTagAIBehaviorConfig",
            AttackCruiserConfigClass::GoToTagAiBehavior => "GotoTagAIBehaviorConfig",
            AttackCruiserConfigClass::GoToTargetAiBehavior => "GotoTargetAIBehaviorConfig",
            AttackCruiserConfigClass::HealthPickup => "HealthPickupConfig",
            AttackCruiserConfigClass::InventoryPickup => "InventoryPickupConfig",
            AttackCruiserConfigClass::KillStreak => "NDConfig",
            AttackCruiserConfigClass::LifePickup => "LifePickupConfig",
            AttackCruiserConfigClass::LifeTimeAiBehavior => "SetLifeTimeAIBehaviorConfig",
            AttackCruiserConfigClass::MultiCondition => "MultiConditionConfig",
            AttackCruiserConfigClass::Path => "PathConfig",
            AttackCruiserConfigClass::Pickup => "PickupConfig",
            AttackCruiserConfigClass::ReturnHomeAiBehavior => "ReturnHomeAIBehaviorConfig",
            AttackCruiserConfigClass::ScorePickup => "ScorePickupConfig",
            AttackCruiserConfigClass::Ship => "ShipConfig",
            AttackCruiserConfigClass::ShipBay => "ShipBayConfig",
            AttackCruiserConfigClass::SimplePhysics => "SimplePhysicsConfig",
            AttackCruiserConfigClass::Squadron => "SquadronConfig",
            AttackCruiserConfigClass::TagPosition => "TagPositionConfig",
            AttackCruiserConfigClass::TargetAiBehavior => "TargetAIBehaviorConfig",
            AttackCruiserConfigClass::Torpedo => "TorpedoConfig",
            AttackCruiserConfigClass::TorpedoBay => "TorpedoBayConfig",
            AttackCruiserConfigClass::Wave => "WaveConfig",
            AttackCruiserConfigClass::WaveTimerAiBehavior => "SetWaveTimerAIBehaviorConfig",
            AttackCruiserConfigClass::WaveVariableAiBehavior => "SetWaveVariableAIBehaviorConfig",
            AttackCruiserConfigClass::WeaponBay => "WeaponBayConfig",
            AttackCruiserConfigClass::WeaponTierPickup => "WeaponTierPickupConfig",
        }
    }
}

pub struct AttackCruiserConfigReference {
    pub class: AttackCruiserConfigClass,
    pub name: String,
}

impl SerializePacket for AttackCruiserConfigReference {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.class.name().serialize(buffer);
        self.name.serialize(buffer);
    }
}

pub struct AttackCruiserConfigHash {
    pub name: String,
    pub class: AttackCruiserConfigClass,
}

impl SerializePacket for AttackCruiserConfigHash {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        hash_string(&self.name).serialize(buffer);
        self.class.serialize(buffer);
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserHudMessageConfig {
    pub speaker_name_id: i32,
    pub speaker_image_id: i32,
    pub message_id: i32,
    pub sound_id: i32,
    pub duration_millis: u32,
    pub delay_millis: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserSpawnConfig {
    pub actor_config: AttackCruiserConfigReference,
    pub ai_config: AttackCruiserConfigReference,
    pub chance: f32,
    pub forward_velocity: f32,
    pub count: u32,
    pub lifespan: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserDeathSpawnConfig {
    pub enable_chance: f32,
    pub spawn_config: AttackCruiserVec<AttackCruiserSpawnConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserGlobalConfig {
    pub physics_speed: f32,
    pub connect_timeout_seconds: f32,
    pub ready_timeout_seconds: f32,
    pub default_timeout_seconds: f32,
    pub effects_preload_timeout_seconds: f32,
    pub effects_ready_timeout_seconds: f32,
    pub server_update_players_interval_seconds: f32,
    pub server_update_actors_interval_seconds: f32,
    pub server_draw_debug_data_interval_seconds: f32,
    pub client_update_actors_interval_seconds: f32,
    pub max_interpolation_step: f32,
    pub small_mass_threshold: f32,
    pub dodge_prediction_time: f32,
    pub dodge_separation: f32,
    pub player_perfect_aim_radius: f32,
    pub player_auto_aim_assistance: f32,
    pub npc_auto_aim_assistance: f32,
    pub player_blaster_trapezoid_width: f32,
    pub player_auto_aim_range: f32,
    pub npc_auto_aim_range: f32,
    pub player_blaster_vertical_range: f32,
    pub npc_blaster_vertical_range: f32,
    pub min_blaster_speed: f32,
    pub max_blaster_angle: f32,
    pub projectile_ray_advance_seconds: f32,
    pub projectile_ray_spacing: f32,
    pub projectile_ray_iterations: i32,
    pub advance_launch_seconds: f32,
    pub advance_interception_time: f32,
    pub collisionless_time: u32,
    pub tractionless_time: u32,
    pub screen_relative_turning: AttackCruiserBool,
    pub ship_to_ship_collision: AttackCruiserBool,
    pub player_death_animation_delay_seconds: f32,
    pub respawn_damage_area: f32,
    pub respawn_delay_seconds: f32,
    pub respawn_invulnerable_seconds: f32,
    pub enable_composite_effects: AttackCruiserBool,
    pub torpedo_reticule_effect_id: u32,
    pub torpedo_reticule_effect_seconds: f32,
    pub fighter_reticule_effect_id: u32,
    pub fighter_reticule_effect_seconds: f32,
    pub wave_end_sound_id: u32,
    pub damage_warning_sound_id: u32,
    pub damage_warning_interval_seconds: f32,
    pub mine_deploy_sound_id: u32,
    pub fighter_launch_sound_id: u32,
    pub score_meter_tier1: u32,
    pub score_decay_tier1: u32,
    pub score_meter_exponent: f32,
    pub score_decay_exponent: f32,
    pub health_foreground_image_id: u32,
    pub health_background_image_id: u32,
    pub health_foreground_internal_id: i32,
    pub health_background_internal_id: i32,
    pub enable_weapon_tiers: AttackCruiserBool,
    pub player_death_spawn_config: AttackCruiserConfigReference,
    pub hud_message: AttackCruiserHudMessageConfig,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlanetConfig {
    pub model_id: u32,
    pub pos: Pos3,
    pub rotation_speed: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWeaponBayConfig {
    pub exit_velocity: f32,
    pub life_time_seconds: f32,
    pub reload_time_seconds: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserBlasterConfig {
    pub weapon_bay_config: AttackCruiserWeaponBayConfig,
    pub blaster_effect_id: u32,
    pub impact_effect_id: u32,
    pub collision_damage: u32,
    pub width: f32,
    pub length: f32,
    pub auto_fire: AttackCruiserBool,
    pub auto_fire_range: f32,
    pub penetrate: AttackCruiserBool,
}

#[derive(SerializePacket)]
pub struct AttackCruiserShipWeaponConfig {
    pub weapon_bay_config: AttackCruiserConfigReference,
    pub group: u32,
    pub tier: i32,
    pub special_weapon: AttackCruiserBool,
    pub exit_offset_x: f32,
    pub exit_offset_y: f32,
    pub exit_offset_z: f32,
    pub exit_offset_angle: f32,
    pub exit_min_angle: f32,
    pub exit_max_angle: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserShipConfig {
    pub actor_config: AttackCruiserActorConfig,
    pub thruster_effect_id: u32,
    pub invulnerable_effect_id: u32,
    pub stun_effect_id: u32,
    pub weapons: AttackCruiserVec<AttackCruiserShipWeaponConfig>,
    pub roll_max_angle: f32,
    pub pitch_max_angle: f32,
    pub continuous_fire_seconds: f32,
    pub fire_cooldown_seconds: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserCameraConfig {
    pub distance: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub pitch: f32,
    pub min_pitch: f32,
    pub max_pitch: f32,
    pub z_offset: f32,
    pub target_tracking_hlq: f32,
    pub zoom_step_q: f32,
    pub zoom_step_hlq: f32,
    pub forward_tether: AttackCruiserBool,
    pub forward_tether_seconds: f32,
    pub near_clip_distance: f32,
    pub particle_update_distance: f32,
    pub actor_update_radius: f32,
    pub shadow_quality: i32,
    pub shadow_draw_distance: f32,
    pub shadow_blob_render_distance: f32,
    pub overhead_render_distance: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserEventCinematicConfig {
    pub total_seconds: f32,
    pub animation_id: i32,
    pub camera_heading: f32,
    pub camera_fov: f32,
    pub flip_camera_z: AttackCruiserBool,
    pub pre_wipe_style: i32,
    pub post_wipe_style: i32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserEventActorConfig {
    pub model_id: u32,
    pub animation_id: i32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerConfig {
    pub ship_config: AttackCruiserConfigReference,
    pub camera_config: AttackCruiserConfigReference,
    pub lives: u32,
    pub spawn_pos: Pos3,
    pub spawn_heading: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserEventConfig {
    pub event_type: i32,
    pub cinematics: AttackCruiserVec<AttackCruiserEventCinematicConfig>,
    pub event_actors: AttackCruiserVec<AttackCruiserEventActorConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserActorAnimationConfig {
    pub animation_type: i32,
    pub slot_id: u32,
    pub loops: AttackCruiserBool,
    pub play_time_seconds: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserActorCinematicConfig {
    pub cinematic_type: i32,
    pub play_time_seconds: f32,
    pub animation_id: u32,
    pub pre_wipe_style: i32,
    pub post_wipe_style: i32,
    pub post_camera_ease_in_seconds: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserActorDamageStateEffectConfig {
    pub effect_id: u32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub offset_z: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserActorDamageStateConfig {
    pub min_health_percent: f32,
    pub texture_alias: String,
    pub effects: AttackCruiserVec<AttackCruiserActorDamageStateEffectConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserBasePhysicsConfig {
    pub contact_response: AttackCruiserBool,
    pub mass: f32,
    pub length: f32,
    pub width: f32,
    pub height: f32,
    pub center_of_mass_z: f32,
    pub max_speed: f32,
    pub vertical_speed: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserSimplePhysicsFlightConfig {
    pub acceleration: f32,
    pub deceleration: f32,
    pub base_deceleration: f32,
    pub max_speed: f32,
    pub max_angular_speed: f32,
    pub angular_acceleration: f32,
    pub traction: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserSimplePhysicsConfig {
    pub base_config: AttackCruiserBasePhysicsConfig,
    pub flight_configs: AttackCruiserVec<AttackCruiserSimplePhysicsFlightConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserComplexPhysicsGear {
    pub shift_up_speed: f32,
    pub shift_down_speed: f32,
    pub base_acceleration: f32,
    pub base_deceleration: f32,
    pub turbo_acceleration: f32,
    pub brake_deceleration: f32,
    pub sideways_deceleration: f32,
    pub angular_acceleration: f32,
    pub turbo_angular_acceleration: f32,
    pub angular_deceleration: f32,
    pub max_angular_speed: f32,
    pub turbo_max_angular_speed: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserComplexPhysicsConfig {
    pub base_config: AttackCruiserBasePhysicsConfig,
    pub reverse_speed: f32,
    pub turbo_speed: f32,
    pub stationary_turn: f32,
    pub gears: AttackCruiserVec<AttackCruiserComplexPhysicsGear>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserActorConfig {
    pub model_id: u32,
    pub effect_id: u32,
    pub death_effect_id: u32,
    pub despawn_effect_id: u32,
    pub explode_offset: f32,
    pub collision_asset_name: String,
    pub physics_config: AttackCruiserConfigReference,
    pub max_health: u32,
    pub explosive_collision: AttackCruiserBool,
    pub collision_damage: u32,
    pub score: u32,
    pub bonus_score: u32,
    pub bonus_max_age_seconds: f32,
    pub overhead_offset_y: f32,
    pub overhead_health_scale: f32,
    pub animations: AttackCruiserVec<AttackCruiserActorAnimationConfig>,
    pub cinematics: AttackCruiserVec<AttackCruiserActorCinematicConfig>,
    pub damage_states: AttackCruiserVec<AttackCruiserActorDamageStateConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserActorPoolConfig {
    pub actor_config: AttackCruiserConfigReference,
    pub size: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWaveActorConfig {
    pub actor_config: AttackCruiserConfigReference,
    pub ai_config: AttackCruiserConfigReference,
    pub squadron_config: AttackCruiserConfigReference,
    pub spawn_condition_config: AttackCruiserConfigReference,
    pub launch_time_seconds: f32,
    pub life_time_seconds: f32,
    pub spawn_pos: Pos3,
    pub spawn_heading: f32,
    pub spawn_speed: f32,
    pub is_hidden: AttackCruiserBool,
    pub has_boss: AttackCruiserBool,
    pub death_spawn_condition_config: AttackCruiserConfigReference,
    pub death_spawn_config: AttackCruiserConfigReference,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWaveHudMessageConfig {
    pub display_condition_config: AttackCruiserConfigReference,
    pub hud_message_config: AttackCruiserHudMessageConfig,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWaveConfig {
    pub actors: AttackCruiserVec<AttackCruiserWaveActorConfig>,
    pub hud_messages: AttackCruiserVec<AttackCruiserWaveHudMessageConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserGameWaveConfig {
    pub wave_config: AttackCruiserConfigReference,
    pub launch_condition_config: AttackCruiserConfigReference,
    pub complete_condition_config: AttackCruiserConfigReference,
    pub remove_actors_on_completion: AttackCruiserBool,
}

#[derive(SerializePacket)]
pub struct AttackCruiserGameConfig {
    pub id: i32,
    pub encounter_id: i32,
    pub sound_id: i32,
    pub mode: i32,
    pub global_config: AttackCruiserConfigReference,
    pub end_condition_config: AttackCruiserConfigReference,
    pub win_condition_config: AttackCruiserConfigReference,
    pub target_value1: u32,
    pub target_value2: u32,
    pub playfield_height: f32,
    pub playfield_length: f32,
    pub playfield_width: f32,
    pub playfield_warning_length: f32,
    pub playfield_warning_width: f32,
    pub playfield_center_x: f32,
    pub playfield_center_z: f32,
    pub kill_zone_height: f32,
    pub enemy_attack_radius: f32,
    pub endless_waves: AttackCruiserBool,
    pub debugged_actors: i32,
    pub global_tilt_init_x: f32,
    pub global_tilt_init_z: f32,
    pub global_tilt_rate_x: f32,
    pub global_tilt_rate_z: f32,
    pub planet: AttackCruiserPlanetConfig,
    pub players: AttackCruiserVec<AttackCruiserPlayerConfig>,
    pub events: AttackCruiserVec<AttackCruiserEventConfig>,
    pub actor_pools: AttackCruiserVec<AttackCruiserActorPoolConfig>,
    pub waves: AttackCruiserVec<AttackCruiserGameWaveConfig>,
}

pub enum AttackCruiserConfigDefinition {
    Actor(Box<AttackCruiserActorConfig>),
    Blaster(Box<AttackCruiserBlasterConfig>),
    Camera(Box<AttackCruiserCameraConfig>),
    ComplexPhysics(Box<AttackCruiserComplexPhysicsConfig>),
    DeathSpawn(Box<AttackCruiserDeathSpawnConfig>),
    Game(Box<AttackCruiserGameConfig>),
    Global(Box<AttackCruiserGlobalConfig>),
    Ship(Box<AttackCruiserShipConfig>),
    SimplePhysics(Box<AttackCruiserSimplePhysicsConfig>),
    Wave(Box<AttackCruiserWaveConfig>),
    WeaponBay(Box<AttackCruiserWeaponBayConfig>),
}

impl AttackCruiserConfigDefinition {
    pub fn class(&self) -> AttackCruiserConfigClass {
        match self {
            AttackCruiserConfigDefinition::Actor(_) => AttackCruiserConfigClass::Actor,
            AttackCruiserConfigDefinition::Blaster(_) => AttackCruiserConfigClass::Blaster,
            AttackCruiserConfigDefinition::Camera(_) => AttackCruiserConfigClass::Camera,
            AttackCruiserConfigDefinition::ComplexPhysics(_) => {
                AttackCruiserConfigClass::ComplexPhysics
            }
            AttackCruiserConfigDefinition::DeathSpawn(_) => AttackCruiserConfigClass::DeathSpawn,
            AttackCruiserConfigDefinition::Game(_) => AttackCruiserConfigClass::Game,
            AttackCruiserConfigDefinition::Global(_) => AttackCruiserConfigClass::Global,
            AttackCruiserConfigDefinition::Ship(_) => AttackCruiserConfigClass::Ship,
            AttackCruiserConfigDefinition::SimplePhysics(_) => {
                AttackCruiserConfigClass::SimplePhysics
            }
            AttackCruiserConfigDefinition::Wave(_) => AttackCruiserConfigClass::Wave,
            AttackCruiserConfigDefinition::WeaponBay(_) => AttackCruiserConfigClass::WeaponBay,
        }
    }
}

impl SerializePacket for AttackCruiserConfigDefinition {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        match self {
            AttackCruiserConfigDefinition::Actor(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::Blaster(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::Camera(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::ComplexPhysics(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::DeathSpawn(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::Game(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::Global(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::Ship(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::SimplePhysics(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::Wave(config) => config.serialize(buffer),
            AttackCruiserConfigDefinition::WeaponBay(config) => config.serialize(buffer),
        }
    }
}

pub struct AttackCruiserConfigName {
    pub hash: AttackCruiserConfigHash,
}

impl SerializePacket for AttackCruiserConfigName {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.hash.serialize(buffer);
        self.hash.name.serialize(buffer);
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserConfig {
    name: AttackCruiserConfigName,
    definition: AttackCruiserConfigDefinition,
}

impl AttackCruiserConfig {
    pub fn new(name: String, definition: AttackCruiserConfigDefinition) -> Self {
        AttackCruiserConfig {
            name: AttackCruiserConfigName {
                hash: AttackCruiserConfigHash {
                    name,
                    class: definition.class(),
                },
            },
            definition,
        }
    }
}

pub struct AttackCruiserClientConfig {
    pub minigame_header: MinigameHeader,
    pub config1: AttackCruiserConfig,
    pub config2: AttackCruiserConfig,
    pub config3: AttackCruiserConfig,
    pub configs: Vec<AttackCruiserConfig>,
}

impl SerializePacket for AttackCruiserClientConfig {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.minigame_header.serialize(buffer);
        (self.configs.len() as u32).serialize(buffer);
        self.config1.serialize(buffer);
        self.config2.serialize(buffer);
        self.config3.serialize(buffer);
        self.configs
            .iter()
            .for_each(|config| config.serialize(buffer));
    }
}

impl GamePacket for AttackCruiserClientConfig {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserUpdateGameState {
    pub minigame_header: MinigameHeader,
    pub game_state: u32,
}

impl GamePacket for AttackCruiserUpdateGameState {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

struct AttackCruiserPlayerStateType {
    pub unknown1: bool,
    pub unknown2: bool,
    pub unknown3: bool,
    pub unknown4: bool,
    pub unknown5: bool,
}

impl SerializePacket for AttackCruiserPlayerStateType {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let mut value = 0;
        if self.unknown1 {
            value |= 0b1;
        }
        if self.unknown2 {
            value |= 0b10;
        }
        if self.unknown3 {
            value |= 0b100;
        }
        if self.unknown4 {
            value |= 0b1000;
        }
        if self.unknown5 {
            value |= 0b10000;
        }

        value.serialize(buffer);
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerStateUnknown1 {
    pub player_index: u32,
    pub actor_id: u32,
    pub unknown_value4: u32,
    pub unknown4: String,
    pub unknown5: String,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerStateScore {
    pub score: u32,
    pub score_multiplier_tier_progress: u32,
    pub score_multiplier_tier_goal: u32,
    pub score_multiplier_tier: u32,
    pub unknown5: u32,
    pub lives: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerStateUnknown3 {
    pub actor_id: u32,
    pub unknown_value4: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerStateSpecialWeapon {
    pub unknown1: u32,
    pub unknown2: u32,
    pub quantity: u32,
    pub unknown4: u32,
    pub icon_id: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerStateUnknown5 {
    pub actor_id: u32,
}

pub struct AttackCruiserPlayerState {
    pub unknown1: Option<AttackCruiserPlayerStateUnknown1>,
    pub score: Option<AttackCruiserPlayerStateScore>,
    pub unknown3: Option<AttackCruiserPlayerStateUnknown3>,
    pub special_weapon: Option<AttackCruiserPlayerStateSpecialWeapon>,
    pub unknown5: Option<AttackCruiserPlayerStateUnknown5>,
}

impl SerializePacket for AttackCruiserPlayerState {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let update_type = AttackCruiserPlayerStateType {
            unknown1: self.unknown1.is_some(),
            unknown2: self.score.is_some(),
            unknown3: self.unknown3.is_some(),
            unknown4: self.special_weapon.is_some(),
            unknown5: self.unknown5.is_some(),
        };
        update_type.serialize(buffer);

        if let Some(unknown1) = &self.unknown1 {
            unknown1.serialize(buffer);
        }

        if let Some(unknown2) = &self.score {
            unknown2.serialize(buffer);
        }

        if let Some(unknown3) = &self.unknown3 {
            unknown3.serialize(buffer);
        }

        if let Some(unknown4) = &self.special_weapon {
            unknown4.serialize(buffer);
        }

        if let Some(unknown5) = &self.unknown5 {
            unknown5.serialize(buffer);
        }
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserAddPlayer {
    pub minigame_header: MinigameHeader,
    pub guid: u64,
    pub state: AttackCruiserPlayerState,
}

impl GamePacket for AttackCruiserAddPlayer {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

pub struct AttackCruiserConfigPlayer {
    pub minigame_header: MinigameHeader,
    pub guid: u64,
    pub config1: Option<AttackCruiserConfig>,
    pub config2: Option<AttackCruiserConfig>,
    pub config3: Option<AttackCruiserConfig>,
}

impl SerializePacket for AttackCruiserConfigPlayer {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.minigame_header.serialize(buffer);
        self.guid.serialize(buffer);

        let mut config_buffer = Vec::new();
        let mut config_flags = 0;
        if let Some(config) = &self.config1 {
            config_flags |= 0b1;
            config.serialize(&mut config_buffer);
        }
        if let Some(config) = &self.config2 {
            config_flags |= 0b10;
            config.serialize(&mut config_buffer);
        }
        if let Some(config) = &self.config3 {
            config_flags |= 0b100;
            config.serialize(&mut config_buffer);
        }

        config_flags.serialize(buffer);
        config_buffer.serialize(buffer);
    }
}

impl GamePacket for AttackCruiserConfigPlayer {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(DeserializePacket)]
pub struct AttackCruiserRequestUpdatePlayers {
    pub minigame_header: MinigameHeader,
    pub update_type: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdate {
    pub index: u32,
    pub state: AttackCruiserPlayerState,
}

#[derive(SerializePacket)]
pub struct AttackCruiserUpdatePlayers {
    pub minigame_header: MinigameHeader,
    pub states: Vec<AttackCruiserPlayerUpdate>,
}

impl GamePacket for AttackCruiserUpdatePlayers {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(Debug, Default)]
pub struct AttackCruiserActorState {
    pub unknown1: bool,
    pub unknown2: bool,
    pub invulnerable: bool,
    pub unknown4: bool,
    pub unknown5: bool,
    pub unknown6: bool,
    pub unknown7: bool,
    pub unknown8: bool,
    pub unknown9: bool,
    pub thrusters_flicker: bool,
    pub thrusters_on: bool,
    pub end_game_hyperdrive: bool,
    pub reset_damage_state: bool,
}

impl SerializePacket for AttackCruiserActorState {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let mut state = 0;
        if self.unknown1 {
            state |= 1 << 0;
        }

        if self.unknown2 {
            state |= 1 << 1;
        }

        if self.invulnerable {
            state |= 1 << 2;
        }

        if self.unknown4 {
            state |= 1 << 3;
        }

        if self.unknown5 {
            state |= 1 << 4;
        }

        if self.unknown6 {
            state |= 1 << 5;
        }

        if self.unknown7 {
            state |= 1 << 6;
        }

        if self.unknown8 {
            state |= 1 << 7;
        }

        if self.unknown9 {
            state |= 1 << 8;
        }

        if self.thrusters_flicker {
            state |= 1 << 9;
        }

        if self.thrusters_on {
            state |= 1 << 10;
        }

        if self.end_game_hyperdrive {
            state |= 1 << 11;
        }

        if self.reset_damage_state {
            state |= 1 << 12;
        }

        state.serialize(buffer);
    }
}

impl DeserializePacket for AttackCruiserActorState {
    fn deserialize(
        cursor: &mut std::io::Cursor<&[u8]>,
    ) -> Result<Self, packet_serialize::DeserializePacketError>
    where
        Self: Sized,
    {
        let state = i32::deserialize(cursor)?;
        let unknown1 = state & (1 << 0) != 0;
        let unknown2 = state & (1 << 1) != 0;
        let invulnerable = state & (1 << 2) != 0;
        let unknown4 = state & (1 << 3) != 0;
        let unknown5 = state & (1 << 4) != 0;
        let unknown6 = state & (1 << 5) != 0;
        let unknown7 = state & (1 << 6) != 0;
        let unknown8 = state & (1 << 7) != 0;
        let unknown9 = state & (1 << 8) != 0;
        let thrusters_flicker = state & (1 << 9) != 0;
        let thrusters_on = state & (1 << 10) != 0;
        let end_game_hyperdrive = state & (1 << 11) != 0;
        let reset_damage_state = state & (1 << 12) != 0;

        Ok(AttackCruiserActorState {
            unknown1,
            unknown2,
            invulnerable,
            unknown4,
            unknown5,
            unknown6,
            unknown7,
            unknown8,
            unknown9,
            thrusters_flicker,
            thrusters_on,
            end_game_hyperdrive,
            reset_damage_state,
        })
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserActorUpdate {
    pub actor_id: u32,
    pub pos: Pos,
    pub speed: Pos,
    pub forward_multiplier: f32,
    pub turn_multiplier: f32,
    pub health: u32,
    pub state: AttackCruiserActorState,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserUpdateActors {
    pub minigame_header: MinigameHeader,
    pub states: Vec<AttackCruiserActorUpdate>,
}

impl GamePacket for AttackCruiserUpdateActors {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(Clone, Copy, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket)]
#[repr(i32)]
pub enum AttackCruiserClickType {
    Right = 0x3,
    Left = 0x4,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserClickedLocation {
    pub minigame_header: MinigameHeader,
    pub click_type: AttackCruiserClickType,
    pub clicked_pos: Pos,
}

impl GamePacket for AttackCruiserClickedLocation {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserAddProjectile {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: f32,
    pub unknown6: Pos3,
    pub unknown7: Pos3,
    pub unknown8: Pos3,
    pub unknown9: f32,
    pub unknown10: f32,
    pub unknown11: f32,
    pub unknown12: f32,
    pub unknown13: u32,
}

impl GamePacket for AttackCruiserAddProjectile {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserRemoveProjectile {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: f32,
}

impl GamePacket for AttackCruiserRemoveProjectile {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(SerializePacket)]
pub struct AttackCruiserAddActor {
    pub minigame_header: MinigameHeader,
    pub actor_id: u32,
    pub unknown2: u32,
    pub actor_pool_id: u64,
    pub pos: Pos3,
    pub speed: Pos3,
    pub heading: f32,
    pub unknown7: u32,
}

impl GamePacket for AttackCruiserAddActor {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AttackCruiserRoundTrip {
    pub minigame_header: MinigameHeader,
    pub client_timestamp: u64,
    pub server_timestamp: u64,
}

impl GamePacket for AttackCruiserRoundTrip {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(SerializePacket)]
pub struct AttackCruiserBoolCommand {
    pub guid: u64,
    pub value: bool,
}

#[derive(SerializePacket)]
pub struct AttackCruiserUnknownCommand2 {
    pub guid: u64,
    pub unknown1: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserUnknownCommand3 {
    pub guid: u64,
    pub unknown1: Pos3,
    pub unknown2: Pos3,
    pub unknown3: Pos3,
    pub unknown4: f32,
    pub unknown5: f32,
    pub unknown6: f32,
    pub unknown7: f32,
    pub unknown8: f32,
    pub unknown9: f32,
    pub unknown10: Pos3,
    pub unknown11: Pos3,
    pub unknown12: Pos3,
    pub unknown13: Pos3,
    pub unknown14: Pos3,
}

#[derive(SerializePacket)]
pub struct AttackCruiserUnknownCommand4 {
    pub guid: u64,
    pub unknown1: f32,
}

pub enum AttackCruiserCommand {
    Movable(AttackCruiserBoolCommand),
    UnknownType3(AttackCruiserBoolCommand),
    UnknownType4(AttackCruiserBoolCommand),
    Visible(AttackCruiserBoolCommand),
    UnknownType6(AttackCruiserUnknownCommand2),
    UnknownType7(AttackCruiserUnknownCommand2),
    UnknownType8(AttackCruiserUnknownCommand2),
    UnknownType9(AttackCruiserUnknownCommand3),
    UnknownType10(AttackCruiserUnknownCommand4),
}

impl SerializePacket for AttackCruiserCommand {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        match self {
            AttackCruiserCommand::Movable(command) => {
                2u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::UnknownType3(command) => {
                3u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::UnknownType4(command) => {
                4u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::Visible(command) => {
                5u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::UnknownType6(command) => {
                6u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::UnknownType7(command) => {
                7u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::UnknownType8(command) => {
                8u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::UnknownType9(command) => {
                9u32.serialize(buffer);
                command.serialize(buffer);
            }
            AttackCruiserCommand::UnknownType10(command) => {
                10u32.serialize(buffer);
                command.serialize(buffer);
            }
        }
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserQueueCommand {
    pub minigame_header: MinigameHeader,
    pub actor_id: u32,
    pub command: AttackCruiserCommand,
}

impl GamePacket for AttackCruiserQueueCommand {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

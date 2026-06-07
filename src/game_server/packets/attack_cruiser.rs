use num_enum::{IntoPrimitive, TryFromPrimitive};
use packet_serialize::{DeserializePacket, SerializePacket};

use crate::game_server::packets::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket, Pos3,
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

#[derive(SerializePacket)]
pub struct AttackCruiserAnyConfig {
    pub class: String,
    pub value: String,
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
    pub player_death_spawn_config: AttackCruiserAnyConfig,
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
pub struct AttackCruiserShipWeaponConfig {
    pub weapon_bay_config: AttackCruiserAnyConfig,
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
    pub ship_config: AttackCruiserAnyConfig,
    pub camera_config: AttackCruiserAnyConfig,
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
    pub base_desceleration: f32,
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
pub struct AttackCruiserActorConfig {
    pub model_id: u32,
    pub effect_id: u32,
    pub death_effect_id: u32,
    pub despawn_effect_id: u32,
    pub explode_offset: f32,
    pub collision_asset_name: String,
    pub physics_config: AttackCruiserAnyConfig,
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
    pub actor_config: AttackCruiserAnyConfig,
    pub size: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWaveActorConfig {
    pub actor_config: AttackCruiserAnyConfig,
    pub ai_config: AttackCruiserAnyConfig,
    pub squadron_config: AttackCruiserAnyConfig,
    pub spawn_condition_config: AttackCruiserAnyConfig,
    pub launch_time_seconds: f32,
    pub life_time_seconds: f32,
    pub spawn_pos: Pos3,
    pub spawn_heading: f32,
    pub spawn_speed: f32,
    pub is_hidden: AttackCruiserBool,
    pub has_boss: AttackCruiserBool,
    pub death_spawn_condition_config: AttackCruiserAnyConfig,
    pub death_spawn_config: AttackCruiserAnyConfig,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWaveHudMessageConfig {
    pub display_condition_config: AttackCruiserAnyConfig,
    pub hud_message_config: AttackCruiserHudMessageConfig,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWaveConfig {
    pub actors: AttackCruiserVec<AttackCruiserWaveActorConfig>,
    pub hud_messages: AttackCruiserVec<AttackCruiserWaveHudMessageConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserGameWaveConfig {
    pub wave_config: AttackCruiserAnyConfig,
    pub launch_condition_config: AttackCruiserAnyConfig,
    pub complete_condition_config: AttackCruiserAnyConfig,
    pub remove_actors_on_completion: AttackCruiserBool,
}

#[derive(SerializePacket)]
pub struct AttackCruiserGameConfig {
    pub id: i32,
    pub encounter_id: i32,
    pub sound_id: i32,
    pub mode: i32,
    pub global_config: AttackCruiserAnyConfig,
    pub end_condition_config: AttackCruiserAnyConfig,
    pub win_condition_config: AttackCruiserAnyConfig,
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
    pub planet_tilt_init_x: f32,
    pub planet_tilt_init_z: f32,
    pub planet_tilt_rate_x: f32,
    pub planet_tilt_rate_z: f32,
    pub planet: AttackCruiserPlanetConfig,
    pub players: AttackCruiserVec<AttackCruiserPlayerConfig>,
    pub events: AttackCruiserVec<AttackCruiserEventConfig>,
    pub actor_pools: AttackCruiserVec<AttackCruiserActorPoolConfig>,
    pub waves: AttackCruiserVec<AttackCruiserGameWaveConfig>,
}

pub enum AttackCruiserConfigType {
    Actor(Box<AttackCruiserActorConfig>),
    Camera(Box<AttackCruiserCameraConfig>),
    Game(Box<AttackCruiserGameConfig>),
    Global(Box<AttackCruiserGlobalConfig>),
    Ship(Box<AttackCruiserShipConfig>),
    SimplePhysics(Box<AttackCruiserSimplePhysicsConfig>),
    WeaponBay(Box<AttackCruiserWeaponBayConfig>),
}

impl SerializePacket for AttackCruiserConfigType {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        match self {
            AttackCruiserConfigType::Actor(config) => config.serialize(buffer),
            AttackCruiserConfigType::Camera(config) => config.serialize(buffer),
            AttackCruiserConfigType::Game(config) => config.serialize(buffer),
            AttackCruiserConfigType::Global(config) => config.serialize(buffer),
            AttackCruiserConfigType::Ship(config) => config.serialize(buffer),
            AttackCruiserConfigType::SimplePhysics(config) => config.serialize(buffer),
            AttackCruiserConfigType::WeaponBay(config) => config.serialize(buffer),
        }
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserConfig {
    pub unknown1: i32,
    pub config_type_hash: i32,
    pub config_reference_name: String,
    pub config_type: AttackCruiserConfigType,
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

#[derive(SerializePacket)]
pub struct AttackCruiserAddActor {
    pub minigame_header: MinigameHeader,
    pub actor_id: u32,
    pub unknown2: u32,
    pub actor_pool_id: u64,
    pub pos: Pos3,
    pub roll_speed: Pos3,
    pub unknown6: u32,
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

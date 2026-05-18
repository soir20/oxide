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
    UpdatePlayerStates = 0x6,
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
    pub screen_relative_turning: bool,
    pub ship_to_ship_collision: bool,
    pub player_death_animation_delay_seconds: f32,
    pub respawn_damage_area: f32,
    pub respawn_delay_seconds: f32,
    pub respawn_invulnerable_seconds: f32,
    pub enable_composite_effects: bool,
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
    pub enable_weapon_tiers: bool,
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
pub struct AttackCruiserShipConfig {}

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
    pub forward_tether: bool,
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
    pub flip_camera_z: bool,
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
    pub ship_config: AttackCruiserShipConfig,
    pub camera_config: AttackCruiserCameraConfig,
    pub lives: u32,
    pub spawn_pos: Pos3,
    pub spawn_heading: f32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserEventConfig {
    pub cinematics: Vec<AttackCruiserEventCinematicConfig>,
    pub event_actors: Vec<AttackCruiserEventActorConfig>,
}

#[derive(SerializePacket)]
pub struct AttackCruiserActorConfig {}

#[derive(SerializePacket)]
pub struct AttackCruiserActorPoolConfig {
    pub actor_config: AttackCruiserActorConfig,
    pub size: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserWaveConfig {}

#[derive(SerializePacket)]
pub struct AttackCruiserGameWaveConfig {
    pub wave_config: AttackCruiserWaveConfig,
    pub launch_condition_config: AttackCruiserAnyConfig,
    pub complete_condition_config: AttackCruiserAnyConfig,
    pub remove_actors_on_completion: bool,
}

#[derive(SerializePacket)]
pub struct AttackCruiserGameConfig {
    pub id: i32,
    pub encounter_id: i32,
    pub sound_id: i32,
    pub mode: i32,
    pub global_config: AttackCruiserGlobalConfig,
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
    pub endless_waves: bool,
    pub debugged_actors: i32,
    pub planet_tilt_init_x: f32,
    pub planet_tilt_init_z: f32,
    pub planet_tilt_rate_x: f32,
    pub planet_tilt_rate_z: f32,
    pub planet: AttackCruiserPlanetConfig,
    pub players: Vec<AttackCruiserPlayerConfig>,
    pub events: Vec<AttackCruiserEventConfig>,
    pub actor_pools: Vec<AttackCruiserActorPoolConfig>,
    pub waves: Vec<AttackCruiserGameWaveConfig>,
}

pub enum AttackCruiserConfigType {
    Global {},
}

impl SerializePacket for AttackCruiserConfigType {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        match self {
            AttackCruiserConfigType::Global { .. } => (0..260).for_each(|_| 0u8.serialize(buffer)),
        }
    }
}

#[derive(SerializePacket)]
pub struct AttackCruiserConfig {
    pub unknown1: i32,
    pub unknown2: i32,
    pub unknown3: String,
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

struct AttackCruiserPlayerUpdateType {
    pub unknown1: bool,
    pub unknown2: bool,
    pub unknown3: bool,
    pub unknown4: bool,
    pub unknown5: bool,
}

impl SerializePacket for AttackCruiserPlayerUpdateType {
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
pub struct AttackCruiserPlayerUpdateUnknown1 {
    pub unknown1: u32,
    pub actor_id: u32,
    pub unknown3: u32,
    pub unknown4: String,
    pub unknown5: String,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown2 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown3 {
    pub unknown1: u32,
    pub unknown2: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown4 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket)]
pub struct AttackCruiserPlayerUpdateUnknown5 {
    pub unknown1: u32,
}

pub struct AttackCruiserPlayerUpdate {
    pub unknown1: Option<AttackCruiserPlayerUpdateUnknown1>,
    pub unknown2: Option<AttackCruiserPlayerUpdateUnknown2>,
    pub unknown3: Option<AttackCruiserPlayerUpdateUnknown3>,
    pub unknown4: Option<AttackCruiserPlayerUpdateUnknown4>,
    pub unknown5: Option<AttackCruiserPlayerUpdateUnknown5>,
}

impl SerializePacket for AttackCruiserPlayerUpdate {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let update_type = AttackCruiserPlayerUpdateType {
            unknown1: self.unknown1.is_some(),
            unknown2: self.unknown2.is_some(),
            unknown3: self.unknown3.is_some(),
            unknown4: self.unknown4.is_some(),
            unknown5: self.unknown5.is_some(),
        };
        update_type.serialize(buffer);

        if let Some(unknown1) = &self.unknown1 {
            unknown1.serialize(buffer);
        }

        if let Some(unknown2) = &self.unknown2 {
            unknown2.serialize(buffer);
        }

        if let Some(unknown3) = &self.unknown3 {
            unknown3.serialize(buffer);
        }

        if let Some(unknown4) = &self.unknown4 {
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
    pub update: AttackCruiserPlayerUpdate,
}

impl GamePacket for AttackCruiserAddPlayer {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::AttackCruiser;
}

#[derive(SerializePacket)]
pub struct AttackCruiserAddActor {
    pub minigame_header: MinigameHeader,
    pub actor_id: u32,
    pub unknown2: u32,
    pub actor_pool_id: u64,
    pub unknown4: Pos3,
    pub unknown5: Pos3,
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

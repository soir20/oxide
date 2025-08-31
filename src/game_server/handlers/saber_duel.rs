use crate::game_server::packets::{item::WieldType, saber_duel::SaberDuelForcePower};

struct SaberDuelAiForcePower {
    force_power: SaberDuelForcePower,
    weight: u8,
}

struct SaberDuelAi {
    name_id: u32,
    model_id: u32,
    wield_type: WieldType,
    entrance_animation_id: u32,
    entrance_sound_id: u32,
    bout_won_sound_id: u32,
    bout_lost_sound_id: u32,
    game_won_sound_id: u32,
    game_lost_sound_id: u32,
    millis_per_key: u16,
    mistake_probability: f32,
    force_power_probability: f32,
    force_powers: Vec<SaberDuelAiForcePower>,
}

impl Default for SaberDuelAi {
    fn default() -> Self {
        Self {
            name_id: Default::default(),
            model_id: Default::default(),
            wield_type: WieldType::SingleSaber,
            entrance_animation_id: Default::default(),
            entrance_sound_id: Default::default(),
            bout_won_sound_id: Default::default(),
            bout_lost_sound_id: Default::default(),
            game_won_sound_id: Default::default(),
            game_lost_sound_id: Default::default(),
            millis_per_key: Default::default(),
            mistake_probability: Default::default(),
            force_power_probability: Default::default(),
            force_powers: Default::default(),
        }
    }
}

struct SaberDuelAppliedForcePower {
    force_power: SaberDuelForcePower,
    bouts_remaining: u8,
}

struct SaberDuelPlayerState {
    ready: bool,
    rounds_won: u8,
    bouts_won: u8,
    progress: u8,
    affected_by_force_powers: Vec<SaberDuelAppliedForcePower>,
    saw_force_power_tutorial: bool,
}

struct SaberDuelAnimationPair {
    attack_animation_id: u32,
    defend_animation_id: u32,
    weight: u8,
}

pub struct SaberDuelGame {
    rounds_to_win: u8,
    bouts_to_win_round: u8,
    keys_per_short_bout: u8,
    keys_per_long_bout: u8,
    first_long_bout: u8,
    long_bout_interval: u8,
    bout_max_millis: u32,
    short_bout_animations: Vec<SaberDuelAnimationPair>,
    long_bout_animations: Vec<SaberDuelAnimationPair>,
    establishing_animation_id: u32,
    player_entrance_animation_id: u32,
    player1: u32,
    player2: Option<u32>,
    player_states: [SaberDuelPlayerState; 2],
    bout: u8,
    ai: SaberDuelAi,
    max_force_points: u8,
    force_points_per_bout_won: u8,
    force_points_per_bout_tied: u8,
    force_points_per_bout_lost: u8,
    force_power_tutorial: Option<SaberDuelForcePower>,
    right_to_left_ai_mistake_multiplier: f32,
    opposite_ai_mistake_multiplier: f32,
    memory_challenge: bool,
}

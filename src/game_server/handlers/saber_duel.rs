use crate::game_server::packets::saber_duel::SaberDuelForcePower;

struct SaberDuelAiForcePower {
    force_power: SaberDuelForcePower,
    weight: u8,
}

#[derive(Default)]
struct SaberDuelAi {
    entrance_animation_id: u32,
    millis_per_key: u16,
    mistake_probability: f32,
    force_power_probability: f32,
    force_powers: Vec<SaberDuelAiForcePower>,
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

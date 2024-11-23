use std::time::{SystemTime, UNIX_EPOCH};

use crate::game_server::packets::time::GameTimeSync;

const SECONDS_PER_REAL_DAY: u32 = 86400;

pub fn make_game_time_sync(seconds_per_day: u32) -> GameTimeSync {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time before Unix epoch")
        .as_secs()
        % seconds_per_day as u64;
    let cycles_per_day = SECONDS_PER_REAL_DAY / seconds_per_day;
    GameTimeSync {
        time: time * cycles_per_day as u64,
        cycles_per_day,
        client_time: false,
    }
}

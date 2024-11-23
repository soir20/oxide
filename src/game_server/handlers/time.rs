use std::time::{SystemTime, UNIX_EPOCH};

use crate::game_server::packets::time::GameTimeSync;

pub fn make_game_time_sync() -> GameTimeSync {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time before Unix epoch")
        .as_secs();
    GameTimeSync {
        time,
        server_rate: 0,
        client_time: true,
    }
}

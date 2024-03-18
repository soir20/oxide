use std::time::{SystemTime, UNIX_EPOCH};
use packet_serialize::{DeserializePacket, SerializePacket};
use crate::game_server::game_packet::{GamePacket, OpCode};

#[derive(SerializePacket, DeserializePacket)]
pub struct GameTimeSync {
    pub time: u64,
    pub unknown1: u32,
    pub unknown2: bool
}

impl GamePacket for GameTimeSync {
    const OP_CODE: OpCode = OpCode::GameTimeSync;
}

pub fn make_game_time_sync() -> GameTimeSync {
    let time = SystemTime::now().duration_since(UNIX_EPOCH)
        .expect("Time before Unix epoch").as_secs();
    GameTimeSync {
        time,
        unknown1: 0,
        unknown2: true,
    }
}

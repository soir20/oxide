use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket};

use crate::game_server::packets::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum DailyMinigameOpCode {
    AddDailyMinigame = 0x1,
    UpdateDailyMinigame = 0x2,
}

impl SerializePacket for DailyMinigameOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::DailyMinigame.serialize(buffer);
        SerializePacket::serialize(&(*self as u8), buffer);
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AddDailyMinigame {
    pub initial_state: UpdateDailyMinigame,
    pub minigame_name: String,
    pub minigame_type: String,
    pub multiplier: f32, // unconfirmed
}

impl GamePacket for AddDailyMinigame {
    type Header = DailyMinigameOpCode;

    const HEADER: Self::Header = DailyMinigameOpCode::AddDailyMinigame;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateDailyMinigame {
    pub guid: u32,
    pub playthroughs_remaining: u32,
    pub consecutive_playthroughs_remaining: u32, // unconfirmed
    pub seconds_until_next_playthrough: u32,
    pub seconds_until_reset: u32,
}

impl GamePacket for UpdateDailyMinigame {
    type Header = DailyMinigameOpCode;

    const HEADER: Self::Header = DailyMinigameOpCode::UpdateDailyMinigame;
}

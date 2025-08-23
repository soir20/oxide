use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, DeserializePacketError, SerializePacket};

use super::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket, Pos,
};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(i32)]
pub enum SaberDuelOpCode {
    StageData = 0x1,
    OpponentGuid = 0x2,
    GameStart = 0x3,
    RoundInfo = 0x4,
    ShowForcePowerDialog = 0x5,
    ApplyForcePower = 0x6,
    RoundStart = 0x8,
    PlayerUpdate = 0x9,
    BoutWon = 0xa,
    BoutTied = 0xb,
    RoundOver = 0xc,
    GameOver = 0xd,
    PlayerReady = 0x10,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelForcePowerDefinition {
    pub guid: u32,
    pub name_id: u32,
    pub small_icon_set_id: u32,
    pub icon_set_id: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SaberDuelStageData {
    pub minigame_header: MinigameHeader,
    pub bouts_to_win_round: u32,
    pub total_rounds: u32,
    pub seconds_remaining: u32,
    pub camera_position: Pos,
    pub camera_rotation: f32,
    pub max_combo_points: u32,
    pub establishing_animation_id: i32,
    pub local_player_index: u32,
    pub opponent_guid: u64,
    pub opponent_entrance_animation_id: i32,
    pub opponent_entrance_sound_id: u32,
    pub max_force_points: u32,
    pub unknown13: bool,
    pub enable_memory_challenge: bool,
    pub force_powers: Vec<SaberDuelForcePowerDefinition>,
}

impl GamePacket for SaberDuelStageData {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct OpponentGuid {
    pub minigame_header: MinigameHeader,
    pub guid: u64,
}

impl GamePacket for OpponentGuid {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct GameStart {
    pub minigame_header: MinigameHeader,
}

impl GamePacket for GameStart {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RoundInfo {
    pub minigame_header: MinigameHeader,
    pub max_round_time_millis: u32,
    pub is_combo_round: bool,
    pub force_points_by_player_index: Vec<u32>,
}

impl GamePacket for RoundInfo {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ShowForcePowerDialog {
    pub minigame_header: MinigameHeader,
    pub unknown1: bool,
}

impl GamePacket for ShowForcePowerDialog {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ApplyForcePower {
    pub minigame_header: MinigameHeader,
    pub used_by_player_index: u32,
    pub force_power_index: u32,
    pub unknown3: u32,
    pub new_force_points: u32,
    pub animation_id: u32,
    pub unknown6: u8,
}

impl GamePacket for ApplyForcePower {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(Clone, Copy, TryFromPrimitive)]
#[repr(u32)]
pub enum SaberDuelKey {
    Up = 1,
    Down = 2,
    Left = 3,
    Right = 4,
}

impl SerializePacket for SaberDuelKey {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        SerializePacket::serialize(&(*self as u32), buffer);
    }
}

impl DeserializePacket for SaberDuelKey {
    fn deserialize(
        cursor: &mut std::io::Cursor<&[u8]>,
    ) -> Result<Self, packet_serialize::DeserializePacketError>
    where
        Self: Sized,
    {
        SaberDuelKey::try_from_primitive(u32::deserialize(cursor)?)
            .map_err(|_| DeserializePacketError::UnknownDiscriminator)
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RoundStart {
    pub minigame_header: MinigameHeader,
    pub keys: Vec<SaberDuelKey>,
    pub num_keys_by_player_index: Vec<u32>,
}

impl GamePacket for RoundStart {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PlayerUpdate {
    pub minigame_header: MinigameHeader,
    pub player_index: u32,
    pub current_key_index: u32,
}

impl GamePacket for PlayerUpdate {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct BoutWon {
    pub minigame_header: MinigameHeader,
    pub winner_index: u32,
    pub new_score: u32,
    pub winner_animation_id: u32,
    pub loser_animation_id: u32,
}

impl GamePacket for BoutWon {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct BoutTied {
    pub minigame_header: MinigameHeader,
}

impl GamePacket for BoutTied {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}


#[derive(SerializePacket, DeserializePacket)]
pub struct RoundOver {
    pub minigame_header: MinigameHeader,
    pub winner_index: u32,
    pub unknown2: u32,
}

impl GamePacket for RoundOver {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct GameOver {
    pub minigame_header: MinigameHeader,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: bool,
    pub unknown4: bool,
}

impl GamePacket for GameOver {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::SaberDuel;
}

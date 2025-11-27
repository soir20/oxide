use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use packet_serialize::{DeserializePacket, SerializePacket};
use rand_distr::{Distribution, Standard};
use serde::Deserialize;

use super::{
    minigame::{MinigameHeader, MinigameOpCode},
    GamePacket, Pos, Target,
};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(i32)]
pub enum TowerDefenseOpCode {
    StageData = 0x1,
    Deck = 0x2,
    Waves = 0x3,
    State = 0x4,
    Notify = 0x6,
    Unknown = 0x9,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownRDArray1 {
    pub guid: u32,
    pub guid2: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: f32,
    pub unknown8: f32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: f32,
    pub unknown12: bool,
    pub unknown13: bool,
    pub unknown14: bool,
    pub unknown15: bool,
    pub unknown16: bool,
    pub unknown17: u32,
    pub unknown18: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownRDArray2 {
    pub guid: u32,
    pub guid2: u32,
    pub unknown2: u32,
    pub unknown3: f32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: bool,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct TowerDefenseStageData {
    pub minigame_header: MinigameHeader,
    pub unknown_header_int: u32,
    pub unknown_header_boolean: bool,
    pub unknown_array1: Vec<UnknownRDArray1>,
    pub unknown_array2: Vec<UnknownRDArray2>,
    pub unknown_pos1: Pos,
    pub unknown_pos2: Pos,
    pub unknown1: f32,
    pub unknown_pos3: Pos,
    pub unknown_pos4: Pos,
}

impl GamePacket for TowerDefenseStageData {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownDeckArray {
    pub unknown1: u32,
    pub unknown2: bool,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct TowerDefenseDeck {
    pub minigame_header: MinigameHeader,
    pub unknown_header_int: u32,
    pub unknown_header_boolean: bool,
    pub unknown1: Vec<UnknownDeckArray>,
    pub unknown2: Vec<UnknownDeckArray>,
}

impl GamePacket for TowerDefenseDeck {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownWaveArray1 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownWaveArray2 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownWaveArray3 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct TowerDefenseWaves {
    pub minigame_header: MinigameHeader,
    pub unknown_header_int: u32,
    pub unknown_header_boolean: bool,
    pub unknown_array1: Vec<UnknownWaveArray1>,
    pub unknown_array2: Vec<UnknownWaveArray2>,
    pub unknown_array3: Vec<UnknownWaveArray3>,
}

impl GamePacket for TowerDefenseWaves {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct TowerDefenseState {
    pub minigame_header: MinigameHeader,
    pub unknown_header_int: u32,
    pub unknown_header_boolean: bool,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

impl GamePacket for TowerDefenseState {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket)]
pub struct TowerDefenseNotify {
    pub minigame_header: MinigameHeader,
    pub unknown_header_int: u32,
    pub unknown_header_boolean: bool,
    pub unknown1: u32,
    pub target: Target,
    pub unknown2: u32,
}

impl GamePacket for TowerDefenseNotify {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket)]
pub struct TowerDefenseUnknown {
    pub minigame_header: MinigameHeader,
    pub unknown_header_int: u32,
    pub unknown_header_boolean: bool,
}

impl GamePacket for TowerDefenseUnknown {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

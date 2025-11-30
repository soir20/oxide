use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket};

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
    TowerTransaction = 0x5,
    Notify = 0x6,
    Unknown = 0x9,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct TowerDefenseTowerDefinition {
    pub guid: u32,
    pub guid2: u32,
    pub rank: u32,
    pub name_id: u32,
    pub tower_type: u32,
    pub energy_cost: u32,
    pub sell_value: u32,
    pub damage: f32,
    pub range: f32,
    pub upgraded_tower_guid: u32,
    pub icon_id: u32,
    pub firing_rate: f32,
    pub can_attack_aerial: bool,
    pub can_attack_ground: bool,
    pub unknown14: bool,
    pub required: bool,
    pub unknown16: bool,
    pub description_id: u32,
    pub shield_damage: u32,
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
    pub sub_op_code: u32,
    pub unknown_header_boolean: bool,
    pub unknown_array1: Vec<TowerDefenseTowerDefinition>,
    pub unknown_array2: Vec<UnknownRDArray2>,
    pub fixed_camera_pos: Pos,
    pub fixed_look_at: Pos,
    pub fixed_field_of_view: f32,
    pub pan_origin: Pos,
    pub pan_max_scale: Pos,
}

impl GamePacket for TowerDefenseStageData {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownDeckArray {
    pub tower_guid: u32,
    pub unknown2: bool,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct TowerDefenseDeck {
    pub minigame_header: MinigameHeader,
    pub sub_op_code: u32,
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
    pub guid: u32,
    pub guid2: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownWaveArray2 {
    pub guid: u32,
    pub guid2: u32,
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
    pub guid: u32,
    pub guid2: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct TowerDefenseWaves {
    pub minigame_header: MinigameHeader,
    pub sub_op_code: u32,
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
    pub sub_op_code: u32,
    pub unknown_header_boolean: bool,
    pub energy: u32,
    pub score: u32,
    pub current_wave: u32,
    pub unknown4: u32,
    pub max_waves: u32,
    pub lives: u32,
}

impl GamePacket for TowerDefenseState {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket)]
pub struct TowerDefenseNotify {
    pub minigame_header: MinigameHeader,
    pub sub_op_code: u32,
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
    pub sub_op_code: u32,
    pub unknown_header_boolean: bool,
}

impl GamePacket for TowerDefenseUnknown {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

#[derive(SerializePacket)]
pub struct TowerTransaction {
    pub minigame_header: MinigameHeader,
    pub sub_op_code: u32,
    pub unknown_header_boolean: bool,
    pub tower_definition_guid: u32,
    pub tower_npc_guid: u64,
    pub base_guid: u64,
    pub unknown4: u64,
    pub unknown5: String,
    pub unknown6: u32,
}

impl GamePacket for TowerTransaction {
    type Header = MinigameOpCode;

    const HEADER: Self::Header = MinigameOpCode::TowerDefense;
}

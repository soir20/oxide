use num_enum::{IntoPrimitive, TryFromPrimitive};
use packet_serialize::{DeserializePacket, SerializePacket};

use crate::game_server::packets::player_data::ActionBarType;

use super::{
    item::{Attachment, EquipmentSlot, Item, ItemDefinition},
    GamePacket, OpCode, Pos,
};

#[derive(Copy, Clone, Debug)]
pub enum ClientUpdateOpCode {
    Health = 0x1,
    AddItems = 0x2,
    EquipItem = 0x5,
    UnequipItem = 0x6,
    Position = 0xc,
    Power = 0xd,
    Stats = 0x7,
    UpdateCredits = 0x13,
    UpdateActionBarSlot = 0x19,
    PreloadCharactersDone = 0x1a,
}

impl SerializePacket for ClientUpdateOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::ClientUpdate.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Position {
    pub player_pos: Pos,
    pub rot: Pos,
    pub is_teleport: bool,
    pub unknown2: bool,
}

impl GamePacket for Position {
    type Header = ClientUpdateOpCode;
    const HEADER: Self::Header = ClientUpdateOpCode::Position;
}

#[derive(SerializePacket)]
pub struct AddItemsData {
    pub item: Item,
    pub definition: ItemDefinition,
}

pub struct AddItems {
    pub data: AddItemsData,
}

impl SerializePacket for AddItems {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let mut inner_buffer = Vec::new();
        self.data.serialize(&mut inner_buffer);
        inner_buffer.serialize(buffer);
    }
}

impl GamePacket for AddItems {
    type Header = ClientUpdateOpCode;
    const HEADER: Self::Header = ClientUpdateOpCode::AddItems;
}

#[derive(SerializePacket)]
pub struct EquipItem {
    pub item_guid: u32,
    pub attachment: Attachment,
    pub battle_class: u32,
    pub item_class: i32,
    pub equip: bool,
}

impl GamePacket for EquipItem {
    type Header = ClientUpdateOpCode;
    const HEADER: Self::Header = ClientUpdateOpCode::EquipItem;
}

#[derive(SerializePacket)]
pub struct UnequipItem {
    pub slot: EquipmentSlot,
    pub battle_class: u32,
}

impl GamePacket for UnequipItem {
    type Header = ClientUpdateOpCode;
    const HEADER: Self::Header = ClientUpdateOpCode::UnequipItem;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

impl GamePacket for Health {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::Health;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Power {
    pub current: u32,
    pub max: u32,
}

impl GamePacket for Power {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::Power;
}

#[derive(
    Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive, SerializePacket, DeserializePacket,
)]
#[repr(u32)]
pub enum StatId {
    MaxHealth = 1,
    Speed = 2,
    Range = 3,
    HealthRegen = 4,
    MaxPower = 5,
    PowerRegen = 6,
    MeleeDefense = 7,
    MeleeDodge = 8,
    MeleeCritRate = 9,
    MeleeCritMultiplier = 10,
    MeleeAccuracy = 11,
    WeaponDamageMultiplier = 12,
    HandToHandDamage = 13,
    WeaponDamage = 14,
    WeaponSpeed = 15,
    DamageReductionFlat = 16,
    ExperienceBoost = 17,
    DamageReductionPct = 18,
    DamageAddition = 19,
    DamageMultiplier = 20,
    HealingAddition = 21,
    HealingMultiplier = 22,
    AbilityCritRate = 33,
    AbilityCritMultiplier = 34,
    Luck = 35,
    HeadInflation = 36,
    CurrencyBoost = 37,
    Toughness = 50,
    AbilityCritVulnerability = 51,
    MeleeCritVulnerability = 52,
    RangeMultiplier = 53,
    MaxShield = 54,
    ShieldRegen = 55,
    MimicMovementSpeed = 57,
    GravityMultiplier = 58,
    JumpHeightMultiplier = 59,
}

#[derive(SerializePacket)]
pub struct Stat {
    pub id: StatId,
    pub multiplier: u32,
    pub value1: f32,
    pub value2: f32,
}

#[derive(SerializePacket)]
pub struct Stats {
    pub stats: Vec<Stat>,
}

impl GamePacket for Stats {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::Stats;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateCredits {
    pub new_credits: u32,
}

impl GamePacket for UpdateCredits {
    type Header = ClientUpdateOpCode;

    const HEADER: Self::Header = ClientUpdateOpCode::UpdateCredits;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateActionBarSlot {
    pub action_bar_type: ActionBarType,
    pub slot_index: u32,
    pub is_empty: bool,
    pub icon_id: u32,
    pub icon_tint_id: u32,
    pub name_id: u32,
    pub ability_type: u32,
    pub ability_sub_type: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub required_force_points: u32,
    pub is_enabled: bool,
    pub use_cooldown_millis: u32,
    pub init_cooldown_millis: u32,
    pub unknown13: u32,
    pub quantity: u32,
    pub is_consumable: bool,
    pub millis_since_last_use: u32,
}

impl GamePacket for UpdateActionBarSlot {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::UpdateActionBarSlot;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PreloadCharactersDone {
    pub unknown1: bool,
}

impl GamePacket for PreloadCharactersDone {
    type Header = ClientUpdateOpCode;
    const HEADER: ClientUpdateOpCode = ClientUpdateOpCode::PreloadCharactersDone;
}

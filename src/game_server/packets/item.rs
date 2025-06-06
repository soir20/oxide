use byteorder::{LittleEndian, ReadBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, DeserializePacketError, SerializePacket};
use serde::{de::IgnoredAny, Deserialize};

use super::{player_update::CustomizationSlot, GamePacket, OpCode};

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, TryFromPrimitive)]
#[serde(deny_unknown_fields)]
#[repr(u32)]
pub enum EquipmentSlot {
    None = 0,
    Head = 1,
    Hands = 2,
    Body = 3,
    Feet = 4,
    Shoulders = 5,
    FacePattern = 6,
    PrimaryWeapon = 7,
    SecondaryWeapon = 8,
    PrimarySaberShape = 10,
    PrimarySaberColor = 11,
    SecondarySaberShape = 12,
    SecondarySaberColor = 13,
    CustomHead = 15,
    CustomHair = 16,
    CustomModel = 17,
    CustomBeard = 18,
}

impl EquipmentSlot {
    pub fn is_weapon(self) -> bool {
        self == EquipmentSlot::PrimaryWeapon || self == EquipmentSlot::SecondaryWeapon
    }

    pub fn is_saber(self) -> bool {
        matches!(
            self,
            EquipmentSlot::PrimaryWeapon
                | EquipmentSlot::SecondaryWeapon
                | EquipmentSlot::PrimarySaberShape
                | EquipmentSlot::PrimarySaberColor
                | EquipmentSlot::SecondarySaberShape
                | EquipmentSlot::SecondarySaberColor
        )
    }
}

impl SerializePacket for EquipmentSlot {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        (*self as u32).serialize(buffer);
    }
}

impl DeserializePacket for EquipmentSlot {
    fn deserialize(
        cursor: &mut std::io::Cursor<&[u8]>,
    ) -> Result<Self, packet_serialize::DeserializePacketError>
    where
        Self: Sized,
    {
        EquipmentSlot::try_from(
            cursor
                .read_u32::<LittleEndian>()
                .map_err(DeserializePacketError::IoError)?,
        )
        .map_err(|_| DeserializePacketError::UnknownDiscriminator)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum WieldType {
    None = 0,
    SingleSaber = 1,
    StaffSaber = 2,
    ReverseSingleSaber = 3,
    DualSaber = 4,
    SinglePistol = 5,
    Rifle = 6,
    SniperRifle = 7,
    RocketLauncher = 8,
    FlameThrower = 9,
    DualPistol = 10,
    Staff = 11,
    Misc = 12,
    Bow = 13,
    Sparklers = 14,
    HeavyCannon = 15,
}

impl SerializePacket for WieldType {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        (*self as u32).serialize(buffer);
    }
}

impl WieldType {
    pub fn holster(&self) -> WieldType {
        match *self {
            WieldType::SingleSaber
            | WieldType::DualSaber
            | WieldType::StaffSaber
            | WieldType::ReverseSingleSaber => WieldType::None,
            _ => *self,
        }
    }
}

#[derive(SerializePacket)]
pub struct Attachment {
    pub model_name: String,
    pub texture_alias: String,
    pub tint_alias: String,
    pub tint: u32,
    pub composite_effect: u32,
    pub slot: EquipmentSlot,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct BaseAttachmentGroup {
    pub unknown1: u32,
    pub unknown2: String,
    pub unknown3: String,
    pub unknown4: u32,
    pub unknown5: String,
}

#[derive(Clone, SerializePacket)]
pub struct Item {
    pub definition_id: u32,
    pub tint: u32,
    pub guid: u32,
    pub quantity: u32,
    pub num_consumed: u32,
    pub last_use_time: u32,
    pub market_data: MarketData,
    pub unknown2: bool,
}

#[derive(Clone)]
pub enum MarketData {
    None,
    #[allow(dead_code)]
    Some(u64, u32, u32),
}

#[derive(Clone, Deserialize, SerializePacket)]
#[serde(deny_unknown_fields)]
pub struct ItemStat {
    #[serde(default)]
    pub comment: IgnoredAny,
}

#[derive(Clone, Deserialize, SerializePacket)]
#[serde(deny_unknown_fields)]
pub struct ItemAbility {
    #[serde(default)]
    pub comment: IgnoredAny,
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
}

#[derive(Clone, Deserialize, SerializePacket)]
#[serde(deny_unknown_fields)]
pub struct ItemDefinition {
    #[serde(default)]
    pub comment: IgnoredAny,
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub tint: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub cost: u32,
    pub item_class: i32,
    pub required_battle_class: u32,
    pub slot: EquipmentSlot,
    pub disable_trade: bool,
    pub disable_sale: bool,
    pub model_name: String,
    pub texture_alias: String,
    pub required_gender: u32,
    pub item_type: u32,
    pub category: u32,
    pub members: bool,
    pub non_minigame: bool,
    pub weapon_trail_effect: u32,
    pub composite_effect: u32,
    pub power_rating: u32,
    pub min_battle_class_level: u32,
    pub rarity: u32,
    pub activatable_ability_id: u32,
    pub passive_ability_id: u32,
    pub single_use: bool,
    pub max_stack_size: i32,
    pub is_tintable: bool,
    pub tint_alias: String,
    pub disable_preview: bool,
    pub unknown33: bool,
    pub race_set_id: u32,
    pub unknown35: bool,
    pub unknown36: u32,
    pub unknown37: u32,
    pub customization_slot: CustomizationSlot,
    pub customization_id: u32,
    pub unknown40: u32,
    pub stats: Vec<ItemStat>,
    pub abilities: Vec<ItemAbility>,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct BrandishHolster {
    pub guid: u64,
}

impl GamePacket for BrandishHolster {
    type Header = OpCode;

    const HEADER: Self::Header = OpCode::BrandishHolster;
}

use crate::game_server::game_packet::GamePacket;
use crate::game_server::player_update_packet::PlayerUpdateOpCode;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{
    DeserializePacket, DeserializePacketError, SerializePacket, SerializePacketError,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Error, Write},
    path::Path,
};

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
    Some(u64, u32, u32),
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, TryFromPrimitive)]
#[repr(u32)]
pub enum EquipmentSlot {
    None = 0,
    Head = 1,
    Hands = 2,
    Body = 3,
    Feet = 4,
    Shoulders = 5,
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

impl SerializePacket for EquipmentSlot {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
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

#[derive(Clone, Deserialize, SerializePacket)]
pub struct ItemStat {}

#[derive(Clone, Deserialize, SerializePacket)]
pub struct ItemAbility {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
}

#[derive(Clone, Deserialize, SerializePacket)]
pub struct ItemDefinition {
    pub guid: u32,
    pub name_id: u32,
    pub description_id: u32,
    pub icon_set_id: u32,
    pub icon_tint: u32,
    pub tint: u32,
    pub unknown7: u32,
    pub cost: u32,
    pub item_class: u32,
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
    pub unknown34: u32,
    pub unknown35: bool,
    pub unknown36: u32,
    pub unknown37: u32,
    pub unknown38: u32,
    pub unknown39: u32,
    pub unknown40: u32,
    pub stats: Vec<ItemStat>,
    pub abilities: Vec<ItemAbility>,
}

pub struct ItemDefinitionsReply<'a> {
    pub definitions: &'a BTreeMap<u32, ItemDefinition>,
}

impl SerializePacket for ItemDefinitionsReply<'_> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();
        self.definitions.serialize(&mut inner_buffer)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;
        Ok(())
    }
}

impl GamePacket for ItemDefinitionsReply<'_> {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::ItemDefinitionsReply;
}

pub fn load_item_definitions(config_dir: &Path) -> Result<BTreeMap<u32, ItemDefinition>, Error> {
    let mut file = File::open(config_dir.join("items.json"))?;
    let item_defs: Vec<ItemDefinition> = serde_json::from_reader(&mut file)?;

    let mut item_def_map = BTreeMap::new();
    for item_def in item_defs {
        if let Some(previous_item_def) = item_def_map.insert(item_def.guid, item_def) {
            panic!("Two item definitions have ID {}", previous_item_def.guid);
        }
    }
    Ok(item_def_map)
}

pub fn load_required_slots(config_dir: &Path) -> Result<BTreeSet<EquipmentSlot>, Error> {
    let mut file = File::open(config_dir.join("required_slots.json"))?;
    let slots: Vec<EquipmentSlot> = serde_json::from_reader(&mut file)?;
    Ok(BTreeSet::from_iter(slots))
}

use std::{collections::BTreeMap, io::Write};

use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{LengthlessVec, SerializePacket, SerializePacketError};

use super::{
    item::{EquipmentSlot, Item, MarketData},
    Effect, GamePacket, ImageId, OpCode, Pos, StringId,
};

#[derive(Clone, SerializePacket)]
pub struct EquippedItem {
    pub slot: EquipmentSlot,
    pub guid: u32,
    pub category: u32,
}

#[derive(Clone, SerializePacket)]
pub struct EquippedVehicle {}

#[derive(Clone, SerializePacket)]
pub struct ItemClassData {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
}

#[derive(Clone, SerializePacket)]
pub struct BattleClassUnknown7 {}

#[derive(Clone)]
pub enum Ability {
    Empty,
    Type1(u32, u32, u32, u32, u32, u32, u32, u32, u32, bool),
    Type2(u32, u32, u32, u32, u32, u32, u32, u32, bool),
    Type3(u32, u32, u32, u32, u32, u32, u32, u32, u32, bool),
    OtherType(u32, u32, u32, u32, u32, u32, u32, u32, bool),
}

impl SerializePacket for Ability {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            Ability::Empty => Ok(buffer.write_u32::<LittleEndian>(0)?),
            Ability::Type1(
                unknown2,
                unknown3,
                unknown5,
                unknown6,
                unknown7,
                unknown8,
                unknown9,
                unknown10,
                unknown11,
                unknown12,
            ) => {
                buffer.write_u32::<LittleEndian>(1)?;
                buffer.write_u32::<LittleEndian>(*unknown2)?;
                buffer.write_u32::<LittleEndian>(*unknown3)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8, *unknown9, *unknown10, *unknown11,
                    *unknown12, buffer,
                )?;
                Ok(())
            }
            Ability::Type2(
                unknown4,
                unknown5,
                unknown6,
                unknown7,
                unknown8,
                unknown9,
                unknown10,
                unknown11,
                unknown12,
            ) => {
                buffer.write_u32::<LittleEndian>(2)?;
                buffer.write_u32::<LittleEndian>(*unknown4)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8, *unknown9, *unknown10, *unknown11,
                    *unknown12, buffer,
                )?;
                Ok(())
            }
            Ability::Type3(
                unknown2,
                unknown3,
                unknown5,
                unknown6,
                unknown7,
                unknown8,
                unknown9,
                unknown10,
                unknown11,
                unknown12,
            ) => {
                buffer.write_u32::<LittleEndian>(3)?;
                buffer.write_u32::<LittleEndian>(*unknown2)?;
                buffer.write_u32::<LittleEndian>(*unknown3)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8, *unknown9, *unknown10, *unknown11,
                    *unknown12, buffer,
                )?;
                Ok(())
            }
            Ability::OtherType(
                unknown1,
                unknown5,
                unknown6,
                unknown7,
                unknown8,
                unknown9,
                unknown10,
                unknown11,
                unknown12,
            ) => {
                buffer.write_u32::<LittleEndian>(*unknown1)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8, *unknown9, *unknown10, *unknown11,
                    *unknown12, buffer,
                )?;
                Ok(())
            }
        }
    }
}

fn write_ability_end(
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
    unknown8: u32,
    unknown9: u32,
    unknown10: u32,
    unknown11: u32,
    unknown12: bool,
    buffer: &mut Vec<u8>,
) -> Result<(), SerializePacketError> {
    buffer.write_u32::<LittleEndian>(unknown5)?;
    buffer.write_u32::<LittleEndian>(unknown6)?;
    buffer.write_u32::<LittleEndian>(unknown7)?;
    buffer.write_u32::<LittleEndian>(unknown8)?;
    buffer.write_u32::<LittleEndian>(unknown9)?;
    buffer.write_u32::<LittleEndian>(unknown10)?;
    buffer.write_u32::<LittleEndian>(unknown11)?;
    buffer.write_u8(unknown12 as u8)?;
    Ok(())
}

#[derive(Clone)]
pub enum BattleClassUnknown10 {
    None,
    Some(u32, bool, u32, u32, u32, u32, u32, u32, u32, u32),
}

impl SerializePacket for BattleClassUnknown10 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            BattleClassUnknown10::None => Ok(buffer.write_u32::<LittleEndian>(0)?),
            BattleClassUnknown10::Some(
                unknown1,
                unknown2,
                unknown3,
                unknown4,
                unknown5,
                unknown6,
                unknown7,
                unknown8,
                unknown9,
                unknown10,
            ) => {
                buffer.write_u32::<LittleEndian>(*unknown1)?;
                buffer.write_u8(*unknown2 as u8)?;
                buffer.write_u32::<LittleEndian>(*unknown3)?;
                buffer.write_u32::<LittleEndian>(*unknown4)?;
                buffer.write_u32::<LittleEndian>(*unknown5)?;
                buffer.write_u32::<LittleEndian>(*unknown6)?;
                buffer.write_u32::<LittleEndian>(*unknown7)?;
                buffer.write_u32::<LittleEndian>(*unknown8)?;
                buffer.write_u32::<LittleEndian>(*unknown9)?;
                buffer.write_u32::<LittleEndian>(*unknown10)?;
                Ok(())
            }
        }
    }
}

#[derive(Clone, SerializePacket)]
pub struct BattleClass {
    pub guid: u32,
    pub name_id: StringId,
    pub description_id: StringId,
    pub selected_ability: u32,
    pub icon_id: ImageId,
    pub unknown1: u32,
    pub badge_background_id: ImageId,
    pub badge_id: ImageId,
    pub members_only: bool,
    pub is_combat: u32,
    pub item_class_data: Vec<ItemClassData>,
    pub unknown2: bool,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: bool,
    pub unknown6: u32,
    pub unknown7: Vec<BattleClassUnknown7>,
    pub level: u32,
    pub xp_in_level: u32,
    pub total_xp: u32,
    pub unknown8: u32,
    pub items: BTreeMap<EquipmentSlot, EquippedItem>,
    pub unknown9: u32,
    pub abilities: Vec<Ability>,
    pub unknown10: LengthlessVec<BattleClassUnknown10>,
}

#[derive(Clone, SerializePacket)]
pub struct Unknown {
    pub unknown1: u32,
    pub unknown2: u32,
}

#[derive(Clone, SerializePacket)]
pub struct SocialInfo {}

impl SerializePacket for MarketData {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        if let MarketData::Some(expiration, upsells, bundle_id) = &self {
            buffer.write_u8(true as u8)?;
            buffer.write_u64::<LittleEndian>(*expiration)?;
            buffer.write_u32::<LittleEndian>(*upsells)?;
            buffer.write_u32::<LittleEndian>(*bundle_id)?;
        } else {
            buffer.write_u8(false as u8)?;
        }
        Ok(())
    }
}

#[derive(Clone, SerializePacket)]
pub struct InventoryItem {
    pub definition_id: u32,
    pub item: Item,
}

#[derive(Clone, SerializePacket)]
pub struct Unknown2 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: bool,
}

#[derive(Clone, SerializePacket)]
pub struct PetTrick {
    pub unknown1: u32,
    pub unknown2: Unknown2,
}

#[derive(Clone, SerializePacket)]
pub struct ItemGuid {
    pub guid: u32,
}

#[derive(Clone, SerializePacket)]
pub struct Item2 {
    pub unknown1: u32,
    pub unknown2: u32,
}

#[derive(Clone, SerializePacket)]
pub struct BattleClassItem {
    pub item1: u32,
    pub item2: Item2,
}

#[derive(Clone, SerializePacket)]
pub struct Unknown12 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

#[derive(Clone, SerializePacket)]
pub struct Unknown13 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
}

#[derive(Clone, SerializePacket)]
pub struct Quest {}

#[derive(Clone, SerializePacket)]
pub struct Achievement {}

#[derive(Clone, SerializePacket)]
pub struct Acquaintance {}

#[derive(Clone, SerializePacket)]
pub struct Recipe {}

#[derive(Clone, SerializePacket)]
pub struct Pet {
    pub pet_id: u32,
    pub unknown2: bool,
    pub unknown3: u32,
    pub food: f32,
    pub groom: f32,
    pub happiness: f32,
    pub exercise: f32,
    pub unknown8: bool,
    pub pet_trick: Vec<PetTrick>,
    pub item_guid: Vec<ItemGuid>,
    pub battle_class_items: Vec<BattleClassItem>,
    pub pet_name: String,
    pub tint_id: u32,
    pub texture_alias: String,
    pub icon_id: u32,
    pub unknown10: bool,
    pub unknown11: u32,
    pub unknown12: Unknown12,
    pub unknown13: Unknown13,
}

#[derive(Clone, SerializePacket)]
pub struct Mount {
    pub mount_id: u32,
    pub name_id: u32,
    pub icon_set_id: u32,
    pub guid: u64,
    pub unknown5: bool,
    pub unknown6: u32,
    pub unknown7: String,
}

#[derive(Clone, SerializePacket)]
pub struct Slot {
    pub slot_id: u32,
    pub empty: bool,
    pub icon_id: ImageId,
    pub unknown1: u32,
    pub name_id: StringId,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub usable: bool,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub quantity: u32,
    pub unknown9: bool,
    pub unknown10: u32,
}

#[derive(Clone, SerializePacket)]
pub struct ActionBar {
    pub unknown1: u32,
    pub unknown2: u32,
    pub slots: Vec<Slot>,
}

pub type MatchmakingQueue = u32;

#[derive(Clone, SerializePacket)]
pub struct MinigameTutorial {}

#[derive(Clone, SerializePacket)]
pub struct PowerHour {}

#[derive(Clone, SerializePacket)]
pub struct Stat {}

#[derive(Clone, SerializePacket)]
pub struct Vehicle {}

#[derive(Clone, SerializePacket)]
pub struct Title {}

#[derive(Clone, SerializePacket)]
pub struct PlayerData {
    pub account_guid: u64,
    pub player_guid: u64,
    pub body_model: u32,
    pub head_model: String,
    pub hair_model: String,
    pub hair_color: u32,
    pub eye_color: u32,
    pub skin_tone: String,
    pub face_paint: String,
    pub facial_hair: String,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub pos: Pos,
    pub rot: Pos,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub first_name: String,
    pub last_name: String,
    pub currency: u32,
    pub account_creation_date: u64,
    pub account_age: u32,
    pub account_play_time: u32,
    pub membership_unknown1: bool,
    pub membership_unknown2: bool,
    pub membership_unknown3: bool,
    pub membership_unknown4: bool,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub unknown13: u32,
    pub unknown14: bool,
    pub unknown15: u32,
    pub unknown16: u32,
    pub equipped_vehicles: Vec<EquippedVehicle>,
    pub battle_classes: BTreeMap<u32, BattleClass>,
    pub active_battle_class: u32,
    pub unknown: Vec<Unknown>,
    pub social: Vec<SocialInfo>,
    pub inventory: Vec<InventoryItem>,
    pub gender: u32,
    pub quests: Vec<Quest>,
    pub quests_unknown1: u32,
    pub quests_unknown2: u32,
    pub quests_unknown3: bool,
    pub quests_unknown4: u32,
    pub quests_unknown5: u32,
    pub achievements: Vec<Achievement>,
    pub acquaintances: Vec<Acquaintance>,
    pub recipes: Vec<Recipe>,
    pub pets: Vec<Pet>,
    pub pet_unknown1: i32,
    pub pet_unknown2: u64,
    pub mounts: Vec<Mount>,
    pub action_bars: Vec<ActionBar>,
    pub unknown17: bool,
    pub matchmaking_queues: Vec<MatchmakingQueue>,
    pub minigame_tutorials: Vec<MinigameTutorial>,
    pub power_hours: Vec<PowerHour>,
    pub stats: Vec<Stat>,
    pub vehicle_unknown1: u32,
    pub vehicles: Vec<Vehicle>,
    pub titles: Vec<Title>,
    pub equipped_title: u32,
    pub unknown18: Vec<u32>,
    pub effects: Vec<Effect>,
}

pub struct Player {
    pub data: PlayerData,
}

impl SerializePacket for Player {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut data_buffer = Vec::new();
        SerializePacket::serialize(&self.data, &mut data_buffer)?;
        buffer.write_u32::<LittleEndian>(data_buffer.len() as u32)?;
        buffer.write_all(&data_buffer)?;
        Ok(())
    }
}

impl GamePacket for Player {
    type Header = OpCode;
    const HEADER: OpCode = OpCode::Player;
}

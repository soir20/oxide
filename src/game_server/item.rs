use std::io::Write;
use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{SerializePacket, SerializePacketError};
use crate::game_server::game_packet::GamePacket;
use crate::game_server::player_update_packet::PlayerUpdateOpCode;

#[derive(SerializePacket)]
pub struct Item {
    pub definition_id: u32,
    pub tint: u32,
    pub guid: u32,
    pub quantity: u32,
    pub num_consumed: u32,
    pub last_use_time: u32,
    pub market_data: MarketData,
    pub unknown2: bool
}

#[derive(Clone)]
pub enum MarketData {
    None,
    Some(u64, u32, u32)
}

#[derive(Copy, Clone, Debug)]
pub enum EquipmentSlot {
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
    CustomBeard = 18
}

impl SerializePacket for EquipmentSlot {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(Clone, SerializePacket)]
pub struct Unknown41 {}

#[derive(Clone, SerializePacket)]
pub struct Unknown42 {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
}

#[derive(Clone, SerializePacket)]
pub struct ItemDefinition {
    guid: u32,
    name_id: u32,
    description_id: u32,
    icon_set_id: u32,
    icon_tint: u32,
    tint: u32,
    unknown7: u32,
    cost: u32,
    class: u32,
    profile_override: u32,
    slot: EquipmentSlot,
    disable_trade: bool,
    disable_sale: bool,
    model_name: String,
    texture_alias: String,
    gender: u32,
    item_type: u32,
    category: u32,
    members: bool,
    non_minigame: bool,
    unknown21: u32,
    unknown22: u32,
    unknown23: u32,
    unknown24: u32,
    unknown25: u32,
    unknown26: u32,
    unknown27: u32,
    unknown28: bool,
    max_stack_size: i32,
    unknown30: bool,
    unknown31: String,
    unknown32: bool,
    unknown33: bool,
    unknown34: u32,
    unknown35: bool,
    unknown36: u32,
    unknown37: u32,
    unknown38: u32,
    unknown39: u32,
    unknown40: u32,
    unknown41: Vec<Unknown41>,
    unknown42: Vec<Unknown42>
}

#[derive(SerializePacket)]
pub struct ItemDefinitionsData {
    pub definitions: Vec<ItemDefinition>
}

pub struct ItemDefinitionsReply {
    pub data: ItemDefinitionsData
}

impl SerializePacket for ItemDefinitionsReply {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();
        self.data.serialize(&mut inner_buffer)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;
        Ok(())
    }
}

impl GamePacket for ItemDefinitionsReply {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::ItemDefinitionsReply;
}

pub fn make_item_definitions() -> ItemDefinitionsReply {
    ItemDefinitionsReply {
        data: ItemDefinitionsData {
            definitions: vec![
                ItemDefinition {
                    guid: 1,
                    name_id: 60676,
                    description_id: 0,
                    icon_set_id: 6953,
                    icon_tint: 0,
                    tint: 0,
                    unknown7: 0,
                    cost: 0,
                    class: 1,
                    profile_override: 0,
                    slot: EquipmentSlot::Head,
                    disable_trade: false,
                    disable_sale: false,
                    model_name: "Wear_Human_<gender>_Head_OfficerCap.adr".to_string(),
                    texture_alias: "OfficerWhite".to_string(),
                    gender: 0,
                    item_type: 1,
                    category: 0,
                    members: false,
                    non_minigame: false,
                    unknown21: 0,
                    unknown22: 0,
                    unknown23: 0,
                    unknown24: 0,
                    unknown25: 0,
                    unknown26: 0,
                    unknown27: 0,
                    unknown28: false,
                    max_stack_size: -1,
                    unknown30: false,
                    unknown31: "".to_string(),
                    unknown32: false,
                    unknown33: false,
                    unknown34: 0,
                    unknown35: false,
                    unknown36: 0,
                    unknown37: 0,
                    unknown38: 0,
                    unknown39: 0,
                    unknown40: 0,
                    unknown41: vec![],
                    unknown42: vec![],
                },
                ItemDefinition {
                    guid: 2,
                    name_id: 60676,
                    description_id: 0,
                    icon_set_id: 6953,
                    icon_tint: 0,
                    tint: 0,
                    unknown7: 0,
                    cost: 0,
                    class: 1,
                    profile_override: 0,
                    slot: EquipmentSlot::Hands,
                    disable_trade: false,
                    disable_sale: false,
                    model_name: "Wear_Human_<gender>_Hands_MandalorianSecretServiceGloves.adr".to_string(),
                    texture_alias: "SecretService".to_string(),
                    gender: 0,
                    item_type: 1,
                    category: 0,
                    members: false,
                    non_minigame: false,
                    unknown21: 0,
                    unknown22: 0,
                    unknown23: 0,
                    unknown24: 0,
                    unknown25: 0,
                    unknown26: 0,
                    unknown27: 0,
                    unknown28: false,
                    max_stack_size: -1,
                    unknown30: false,
                    unknown31: "".to_string(),
                    unknown32: false,
                    unknown33: false,
                    unknown34: 0,
                    unknown35: false,
                    unknown36: 0,
                    unknown37: 0,
                    unknown38: 0,
                    unknown39: 0,
                    unknown40: 0,
                    unknown41: vec![],
                    unknown42: vec![],
                },
                ItemDefinition {
                    guid: 3,
                    name_id: 60676,
                    description_id: 0,
                    icon_set_id: 6953,
                    icon_tint: 0,
                    tint: 0,
                    unknown7: 0,
                    cost: 0,
                    class: 1,
                    profile_override: 0,
                    slot: EquipmentSlot::Body,
                    disable_trade: false,
                    disable_sale: false,
                    model_name: "Wear_Human_<gender>_Body_PulsingCrystalSuit.adr".to_string(),
                    texture_alias: "PulsingCrystalBlue".to_string(),
                    gender: 0,
                    item_type: 1,
                    category: 0,
                    members: false,
                    non_minigame: false,
                    unknown21: 0,
                    unknown22: 0,
                    unknown23: 0,
                    unknown24: 0,
                    unknown25: 0,
                    unknown26: 0,
                    unknown27: 0,
                    unknown28: false,
                    max_stack_size: -1,
                    unknown30: false,
                    unknown31: "".to_string(),
                    unknown32: false,
                    unknown33: false,
                    unknown34: 0,
                    unknown35: false,
                    unknown36: 0,
                    unknown37: 0,
                    unknown38: 0,
                    unknown39: 0,
                    unknown40: 0,
                    unknown41: vec![],
                    unknown42: vec![],
                },
                ItemDefinition {
                    guid: 4,
                    name_id: 60676,
                    description_id: 0,
                    icon_set_id: 6953,
                    icon_tint: 0,
                    tint: 0,
                    unknown7: 0,
                    cost: 0,
                    class: 1,
                    profile_override: 0,
                    slot: EquipmentSlot::Feet,
                    disable_trade: false,
                    disable_sale: false,
                    model_name: "Wear_Human_<gender>_Feet_CloneBoots.adr".to_string(),
                    texture_alias: "ARCFives".to_string(),
                    gender: 0,
                    item_type: 1,
                    category: 0,
                    members: false,
                    non_minigame: false,
                    unknown21: 0,
                    unknown22: 0,
                    unknown23: 0,
                    unknown24: 0,
                    unknown25: 0,
                    unknown26: 0,
                    unknown27: 0,
                    unknown28: false,
                    max_stack_size: -1,
                    unknown30: false,
                    unknown31: "".to_string(),
                    unknown32: false,
                    unknown33: false,
                    unknown34: 0,
                    unknown35: false,
                    unknown36: 0,
                    unknown37: 0,
                    unknown38: 0,
                    unknown39: 0,
                    unknown40: 0,
                    unknown41: vec![],
                    unknown42: vec![],
                },

                ItemDefinition {
                    guid: 5,
                    name_id: 2896,
                    description_id: 0,
                    icon_set_id: 2312,
                    icon_tint: 0,
                    tint: 0,
                    unknown7: 0,
                    cost: 0,
                    class: 1,
                    profile_override: 0,
                    slot: EquipmentSlot::PrimaryWeapon,
                    disable_trade: false,
                    disable_sale: false,
                    model_name: "Wield_Pistol_DC17Chrome.adr".to_string(),
                    texture_alias: "Vigilance".to_string(),
                    gender: 0,
                    item_type: 1,
                    category: 0,
                    members: false,
                    non_minigame: false,
                    unknown21: 0,
                    unknown22: 0,
                    unknown23: 0,
                    unknown24: 0,
                    unknown25: 0,
                    unknown26: 0,
                    unknown27: 0,
                    unknown28: false,
                    max_stack_size: -1,
                    unknown30: false,
                    unknown31: "".to_string(),
                    unknown32: false,
                    unknown33: false,
                    unknown34: 0,
                    unknown35: false,
                    unknown36: 0,
                    unknown37: 0,
                    unknown38: 0,
                    unknown39: 0,
                    unknown40: 0,
                    unknown41: vec![],
                    unknown42: vec![],
                },
            ],
        },
    }
}
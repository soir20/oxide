use std::collections::BTreeMap;
use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};

use packet_serialize::{LengthlessVec, SerializePacket, SerializePacketError};

use crate::game_server::character_guid::{mount_guid, player_guid};
use crate::game_server::game_packet::{Effect, GamePacket, ImageId, OpCode, Pos, StringId};
use crate::game_server::guid::Guid;
use crate::game_server::item::{EquipmentSlot, Item, MarketData};
use crate::game_server::mount::MountConfig;
use crate::game_server::player_update_packet::{Wield, WieldType, NameplateImageId, NameplateImage};
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::zone::{Character, CharacterType};

#[derive(SerializePacket)]
pub struct EquippedVehicle {}

#[derive(SerializePacket)]
pub struct ItemClassData {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
}

#[derive(SerializePacket)]
pub struct ProfileUnknown7 {}

pub enum Ability {
    Empty,
    Type1(u32, u32, u32, u32, u32, u32, u32, u32, u32, bool),
    Type2(u32, u32, u32, u32, u32, u32, u32, u32, bool),
    Type3(u32, u32, u32, u32, u32, u32, u32, u32, u32, bool),
    OtherType(u32, u32, u32, u32, u32, u32, u32, u32, bool)
}

impl SerializePacket for Ability {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            Ability::Empty => Ok(buffer.write_u32::<LittleEndian>(0)?),
            Ability::Type1(unknown2, unknown3, unknown5, unknown6,
                           unknown7, unknown8, unknown9, unknown10,
                           unknown11, unknown12) => {
                buffer.write_u32::<LittleEndian>(1)?;
                buffer.write_u32::<LittleEndian>(*unknown2)?;
                buffer.write_u32::<LittleEndian>(*unknown3)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8,
                    *unknown9, *unknown10, *unknown11, *unknown12,
                    buffer
                )?;
                Ok(())
            },
            Ability::Type2(unknown4, unknown5, unknown6, unknown7,
                           unknown8, unknown9, unknown10, unknown11,
                           unknown12) => {
                buffer.write_u32::<LittleEndian>(2)?;
                buffer.write_u32::<LittleEndian>(*unknown4)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8,
                    *unknown9, *unknown10, *unknown11, *unknown12,
                    buffer
                )?;
                Ok(())
            },
            Ability::Type3(unknown2, unknown3, unknown5, unknown6,
                           unknown7, unknown8, unknown9, unknown10,
                           unknown11, unknown12) => {
                buffer.write_u32::<LittleEndian>(3)?;
                buffer.write_u32::<LittleEndian>(*unknown2)?;
                buffer.write_u32::<LittleEndian>(*unknown3)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8,
                    *unknown9, *unknown10, *unknown11, *unknown12,
                    buffer
                )?;
                Ok(())
            },
            Ability::OtherType(unknown1, unknown5, unknown6, unknown7,
                               unknown8, unknown9, unknown10, unknown11,
                               unknown12) => {
                buffer.write_u32::<LittleEndian>(*unknown1)?;
                write_ability_end(
                    *unknown5, *unknown6, *unknown7, *unknown8,
                    *unknown9, *unknown10, *unknown11, *unknown12,
                    buffer
                )?;
                Ok(())
            },
        }
    }
}

fn write_ability_end(unknown5: u32, unknown6: u32, unknown7: u32, unknown8: u32, unknown9: u32,
                     unknown10: u32, unknown11: u32, unknown12: bool, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
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

pub enum ProfileUnknown10 {
    None,
    Some(u32, bool, u32, u32, u32, u32, u32, u32, u32, u32)
}

impl SerializePacket for ProfileUnknown10 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            ProfileUnknown10::None => Ok(buffer.write_u32::<LittleEndian>(0)?),
            ProfileUnknown10::Some(unknown1, unknown2, unknown3, unknown4,
                                   unknown5, unknown6, unknown7, unknown8,
                                   unknown9, unknown10) => {
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

#[derive(SerializePacket)]
pub struct Profile {
    guid: u32,
    name_id: StringId,
    description_id: StringId,
    selected_ability: u32,
    icon_id: ImageId,
    unknown1: u32,
    badge_background_id: ImageId,
    badge_id: ImageId,
    members_only: bool,
    is_combat: u32,
    item_class_data: Vec<ItemClassData>,
    unknown2: bool,
    unknown3: u32,
    unknown4: u32,
    unknown5: bool,
    unknown6: u32,
    unknown7: Vec<ProfileUnknown7>,
    level: u32,
    xp_in_level: u32,
    total_xp: u32,
    unknown8: u32,
    items: Vec<EquippedItem>,
    unknown9: u32,
    abilities: Vec<Ability>,
    unknown10: LengthlessVec<ProfileUnknown10>
}

#[derive(SerializePacket)]
pub struct EquippedItem {
    slot: EquipmentSlot,
    guid: u32,
    category: u32
}
#[derive(SerializePacket)]
pub struct Unknown {
    pub unknown1: u32,
    pub unknown2: u32
}

#[derive(SerializePacket)]
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

#[derive(SerializePacket)]
pub struct InventoryItem {
    pub definition_id: u32,
    pub item: Item
}

#[derive(SerializePacket)]
struct Unknown2 {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
    unknown8: u32,
    unknown9: bool,
}

#[derive(SerializePacket)]
struct PetTrick {
    unknown1: u32,
    unknown2: Unknown2,
}

#[derive(SerializePacket)]
struct ItemGuid {
    guid: u32,
}

#[derive(SerializePacket)]
struct Item2 {
    unknown1: u32,
    unknown2: u32,
}

#[derive(SerializePacket)]
struct ProfileItem {
    item1: u32,
    item2: Item2,
}

#[derive(SerializePacket)]
struct Unknown12 {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
}

#[derive(SerializePacket)]
struct Unknown13 {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
    unknown8: u32,
}

#[derive(SerializePacket)]
pub struct Quest {}

#[derive(SerializePacket)]
pub struct Achievement {}

#[derive(SerializePacket)]
pub struct Acquaintance {}

#[derive(SerializePacket)]
pub struct Recipe {}

#[derive(SerializePacket)]
pub struct Pet {
	unknown1: u32,
	unknown2: bool,
	unknown3: u32,
	food: f32,
	groom: f32,
	happiness: f32,
	exercise: f32,
	unknown8: bool,
	pet_trick: Vec<PetTrick>,
	item_guid: Vec<ItemGuid>,
	profile_item: Vec<ProfileItem>,
	pet_name: String,
	unknown9: u32,
	texture_alias: String,
	icon_id: u32,
	unknown10: bool,
	unknown11: u32,
	unknown12: Unknown12,
	unknown13: Unknown13,
}

#[derive(SerializePacket)]
pub struct Mount {
    mount_id: u32,
    name_id: u32,
    icon_set_id: u32,
    guid: u64,
    unknown5: bool,
    unknown6: u32,
    unknown7: String,
}

#[derive(SerializePacket)]
pub struct Slot {
    slot_id: u32,
    empty: bool,
    icon_id: ImageId,
    unknown1: u32,
    name_id: StringId,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    usable: bool,
    unknown6: u32,
    unknown7: u32,
    unknown8: u32,
    quantity: u32,
    unknown9: bool,
    unknown10: u32
}

#[derive(SerializePacket)]
pub struct ActionBar {
    unknown1: u32,
    unknown2: u32,
    slots: Vec<Slot>,
}

pub type MatchmakingQueue = u32;

#[derive(SerializePacket)]
pub struct MinigameTutorial {}

#[derive(SerializePacket)]
pub struct PowerHour {}

#[derive(SerializePacket)]
pub struct Stat {}

#[derive(SerializePacket)]
pub struct Vehicle {}

#[derive(SerializePacket)]
pub struct Title {}

#[derive(SerializePacket)]
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
    pub profiles: Vec<Profile>,
    pub active_profile: u32,
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
    pub data: PlayerData
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

pub fn make_test_player(guid: u32, mounts: &BTreeMap<u32, MountConfig>) -> Player {
    let mut owned_mounts = Vec::new();
    for mount in mounts.values() {
        owned_mounts.push(Mount {
            mount_id: mount.guid(),
            name_id: mount.name_id,
            icon_set_id: mount.icon_set_id,
            guid: mount_guid(guid, mount.guid()),
            unknown5: false,
            unknown6: 0,
            unknown7: "".to_string(),
        })
    }

    Player {
        data: PlayerData {
            account_guid: 0,
            player_guid: player_guid(guid),
            body_model: 484,
            head_model: String::from("Char_CloneHead.adr"),
            hair_model: String::from("Cust_Clone_Hair_BusinessMan.adr"),
            hair_color: 11,
            eye_color: 0,
            skin_tone: String::from("CloneTan"),
            face_paint: String::from("SquarishTattoo"),
            facial_hair: String::from(""),
            unknown1: 1,
            unknown2: 5,
            unknown3: 3,
            unknown4: 0,
            unknown5: 0,
            pos: Pos {
                x: 887.3,
                y: 171.93376,
                z: 1546.956,
                w: 1.0,
            },
            rot: Pos {
                x: 1.5,
                y: 0.0,
                z: 0.0,
                w: 0.0,
            },
            unknown6: 0,
            unknown7: 0,
            unknown8: 0,
            first_name: String::from("BLASTER"),
            last_name: String::from("NICESHOT"),
            currency: 0,
            account_creation_date: 1261854072,
            account_age: 0,
            account_play_time: 0,
            membership_unknown1: true,
            membership_unknown2: true,
            membership_unknown3: true,
            membership_unknown4: true,
            unknown9: 217,
            unknown10: 2,
            unknown11: 0,
            unknown12: 0,
            unknown13: 1,
            unknown14: false,
            unknown15: 3,
            unknown16: 5,
            equipped_vehicles: vec![],
            profiles: vec![
                Profile {
                    guid: 1,
                    name_id: 52577,
                    description_id: 2837,
                    selected_ability: 0,
                    icon_id: 6442,
                    unknown1: 0,
                    badge_background_id: 0,
                    badge_id: 0,
                    members_only: false,
                    is_combat: 1,
                    item_class_data: vec![],
                    unknown2: false,
                    unknown3: 0,
                    unknown4: 1931819892,
                    unknown5: false,
                    unknown6: 0,
                    unknown7: vec![],
                    level: 1,
                    xp_in_level: 0,
                    total_xp: 0,
                    unknown8: 0,
                    items: vec![
                        EquippedItem {
                            slot: EquipmentSlot::Head,
                            guid: 1,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::Hands,
                            guid: 2,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::Body,
                            guid: 3,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::Feet,
                            guid: 4,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::Shoulders,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::PrimaryWeapon,
                            guid: 5,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::SecondaryWeapon,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::PrimarySaberShape,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::PrimarySaberColor,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::SecondarySaberShape,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::SecondarySaberColor,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::CustomHead,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::CustomHair,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::CustomModel,
                            guid: 0,
                            category: 0,
                        },
                        EquippedItem {
                            slot: EquipmentSlot::CustomBeard,
                            guid: 0,
                            category: 0,
                        },
                    ],
                    unknown9: 0,
                    abilities: vec![
                        Ability::Empty,
                        Ability::Empty,
                        Ability::Empty,
                        Ability::Empty,
                        Ability::Empty,
                        Ability::Empty,
                        Ability::Empty,
                        Ability::Empty,
                    ],
                    unknown10: LengthlessVec(vec![
                        ProfileUnknown10::None
                    ]),
                }
            ],
            active_profile: 1,
            unknown: vec![],
            social: vec![],
            inventory: vec![
                InventoryItem {
                    definition_id: 1,
                    item: Item {
                        definition_id: 1,
                        tint: 0,
                        guid: 1,
                        quantity: 100,
                        num_consumed: 0,
                        last_use_time: 0,
                        market_data: MarketData::None,
                        unknown2: false,
                    }
                },
                InventoryItem {
                    definition_id: 2,
                    item: Item {
                        definition_id: 2,
                        tint: 0,
                        guid: 2,
                        quantity: 100,
                        num_consumed: 0,
                        last_use_time: 0,
                        market_data: MarketData::None,
                        unknown2: false,
                    }
                },
                InventoryItem {
                    definition_id: 3,
                    item: Item {
                        definition_id: 3,
                        tint: 0,
                        guid: 3,
                        quantity: 100,
                        num_consumed: 0,
                        last_use_time: 0,
                        market_data: MarketData::None,
                        unknown2: false,
                    }
                },
                InventoryItem {
                    definition_id: 4,
                    item: Item {
                        definition_id: 4,
                        tint: 0,
                        guid: 4,
                        quantity: 100,
                        num_consumed: 0,
                        last_use_time: 0,
                        market_data: MarketData::None,
                        unknown2: false,
                    }
                },
                InventoryItem {
                    definition_id: 5,
                    item: Item {
                        definition_id: 5,
                        tint: 0,
                        guid: 5,
                        quantity: 100,
                        num_consumed: 0,
                        last_use_time: 0,
                        market_data: MarketData::None,
                        unknown2: false,
                    }
                },
                InventoryItem {
                    definition_id: 6,
                    item: Item {
                        definition_id: 6,
                        tint: 0,
                        guid: 6,
                        quantity: 100,
                        num_consumed: 50,
                        last_use_time: 0,
                        market_data: MarketData::None,
                        unknown2: false,
                    }
                },
            ],
            gender: 1,
            quests: vec![],
            quests_unknown1: 241,
            quests_unknown2: 2513,
            quests_unknown3: true,
            quests_unknown4: 10,
            quests_unknown5: 30,
            achievements: vec![],
            acquaintances: vec![],
            recipes: vec![],
            pets: vec![
                Pet {
                    unknown1: 0,
                    unknown2: false,
                    unknown3: 0,
                    food: 0.0,
                    groom: 0.0,
                    exercise: 0.0,
                    happiness: 0.0,
                    unknown8: false,
                    pet_trick: vec![
                        PetTrick {
                            unknown1: 0,
                            unknown2: Unknown2 {
                                unknown1: 0,
                                unknown2: 0,
                                unknown3: 0,
                                unknown4: 0,
                                unknown5: 0,
                                unknown6: 0,
                                unknown7: 0,
                                unknown8: 0,
                                unknown9: false,
                            },
                        },
                    ],
                    item_guid: vec![
                        ItemGuid {
                            guid: 0,
                    },
                ],
                    profile_item: vec![
                        ProfileItem {
                            item1: 0,
                            item2: Item2 {
                                unknown1: 0,
                                unknown2: 0,
                            },
                        },
                    ],
                    pet_name: "Test".to_string(),
                    unknown9: 0,
                    texture_alias: "".to_string(),
                    icon_id: 0,
                    unknown10: false,
                    unknown11: 0,
                    unknown12: Unknown12 {
                        unknown1: 0,
                        unknown2: 0,
                        unknown3: 0,
                        unknown4: 0,
                    },
                    unknown13: Unknown13 {
                        unknown1: 0,
                        unknown2: 0,
                        unknown3: 0,
                        unknown4: 0,
                        unknown5: 0,
                        unknown6: 0,
                        unknown7: 0,
                        unknown8: 0,
                    },
                },
            ],
            pet_unknown1: -1,
            pet_unknown2: 0,
            mounts: owned_mounts,
            action_bars: vec![
                ActionBar {
                    unknown1: 2,
                    unknown2: 2,
                    slots: vec![
                        Slot {
                            slot_id: 0,
                            empty: true,
                            icon_id: 0,
                            unknown1: 0,
                            name_id: 0,
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: 0,
                            unknown5: 0,
                            usable: false,
                            unknown6: 0,
                            unknown7: 0,
                            unknown8: 0,
                            quantity: 0,
                            unknown9: false,
                            unknown10: 0,
                        },
                        Slot {
                            slot_id: 1,
                            empty: true,
                            icon_id: 0,
                            unknown1: 0,
                            name_id: 0,
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: 0,
                            unknown5: 0,
                            usable: false,
                            unknown6: 0,
                            unknown7: 0,
                            unknown8: 0,
                            quantity: 0,
                            unknown9: false,
                            unknown10: 0,
                        },
                        Slot {
                            slot_id: 2,
                            empty: true,
                            icon_id: 0,
                            unknown1: 0,
                            name_id: 0,
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: 0,
                            unknown5: 0,
                            usable: false,
                            unknown6: 0,
                            unknown7: 0,
                            unknown8: 0,
                            quantity: 0,
                            unknown9: false,
                            unknown10: 0,
                        },
                        Slot {
                            slot_id: 3,
                            empty: true,
                            icon_id: 0,
                            unknown1: 0,
                            name_id: 0,
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: 0,
                            unknown5: 0,
                            usable: false,
                            unknown6: 0,
                            unknown7: 0,
                            unknown8: 0,
                            quantity: 0,
                            unknown9: false,
                            unknown10: 0,
                        },
                    ],
                }
            ],
            unknown17: false,
            matchmaking_queues: vec![],
            minigame_tutorials: vec![],
            power_hours: vec![],
            stats: vec![],
            vehicle_unknown1: 0,
            vehicles: vec![],
            titles: vec![],
            equipped_title: 0,
            unknown18: vec![],
            effects: vec![],
        }
    }
}

impl From<PlayerData> for Character {
    fn from(value: PlayerData) -> Self {
        Character {
            guid: value.player_guid,
            pos: value.pos,
            rot: value.rot,
            character_type: CharacterType::Player,
            state: 0,
            mount_id: None,
            interact_radius: 0.0,
            auto_interact_radius: 0.0,
        }
    }
}

pub fn make_test_wield_type(guid: u32) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    Ok(
        vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: WieldType {
                    guid: player_guid(guid),
                    wield_type: Wield::SinglePistol,
                },
            })?,
        ]
    )
}
pub fn make_test_nameplate_image(guid: u32) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    Ok(
        vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: NameplateImageId {
                    image_id: NameplateImage::Trooper,
                    guid: player_guid(guid),
                },
            })?,
        ]
    )
}

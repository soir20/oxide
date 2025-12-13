use std::collections::BTreeMap;

use packet_serialize::LengthlessVec;

use crate::game_server::packets::{
    item::{EquipmentSlot, Item, ItemDefinition, MarketData},
    player_data::{
        Ability, ActionBar, BattleClass, BattleClassItem, BattleClassUnknown10, EquippedItem,
        InventoryItem, Item2, ItemGuid, Mount, Pet, PetTrick, Player, PlayerData, Unknown12,
        Unknown13, Unknown2,
    },
    player_update::{CustomizationSlot, NameplateImage, NameplateImageId},
    tunnel::TunneledPacket,
    ActionBarSlot, ActionBarType, GamePacket, Name, Pos,
};

use super::{
    guid::Guid,
    mount::MountConfig,
    unique_guid::{mount_guid, player_guid},
};

pub fn make_test_player(
    guid: u32,
    mounts: &BTreeMap<u32, MountConfig>,
    items: &BTreeMap<u32, ItemDefinition>,
) -> Player {
    let mut owned_mounts = Vec::new();
    for mount in mounts.values() {
        owned_mounts.push(Mount {
            mount_id: mount.guid(),
            name_id: mount.name_id,
            icon_set_id: mount.icon_set_id,
            guid: mount_guid(player_guid(guid)),
            unknown5: false,
            unknown6: 0,
            unknown7: "".to_string(),
        })
    }

    let mut inventory = BTreeMap::new();
    for item in items.values() {
        inventory.insert(
            item.guid,
            InventoryItem {
                definition_id: item.guid,
                item: Item {
                    definition_id: item.guid,
                    tint: item.tint,
                    guid: item.guid,
                    quantity: 1,
                    num_consumed: 0,
                    last_use_time: 0,
                    market_data: MarketData::None,
                    unknown2: false,
                },
            },
        );
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
            head_customization_id: 0,
            hair_style_customization_id: 0,
            skin_tone_customization_id: 0,
            face_design_customization_id: 0,
            model_customization_id: 0,
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
            name: Name {
                first_name_id: 0,
                middle_name_id: 0,
                last_name_id: 0,
                first_name: String::from("BLASTER"),
                last_name: if guid == 1 {
                    String::from("NICESHOT")
                } else {
                    format!("NICESHOT {guid}")
                },
            },
            credits: 1000000,
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
            battle_classes: BTreeMap::from([(
                1,
                BattleClass {
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
                    items: BTreeMap::from([
                        (
                            EquipmentSlot::Head,
                            EquippedItem {
                                slot: EquipmentSlot::Head,
                                guid: 20172,
                                category: 0,
                            },
                        ),
                        (
                            EquipmentSlot::Hands,
                            EquippedItem {
                                slot: EquipmentSlot::Hands,
                                guid: 30167,
                                category: 0,
                            },
                        ),
                        (
                            EquipmentSlot::Body,
                            EquippedItem {
                                slot: EquipmentSlot::Body,
                                guid: 10237,
                                category: 0,
                            },
                        ),
                        (
                            EquipmentSlot::Feet,
                            EquippedItem {
                                slot: EquipmentSlot::Feet,
                                guid: 40065,
                                category: 0,
                            },
                        ),
                        (
                            EquipmentSlot::PrimaryWeapon,
                            EquippedItem {
                                slot: EquipmentSlot::PrimaryWeapon,
                                guid: 110052,
                                category: 0,
                            },
                        ),
                    ]),
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
                    unknown10: LengthlessVec(vec![BattleClassUnknown10::None]),
                },
            )]),
            active_battle_class: 1,
            unknown: vec![],
            social: vec![],
            inventory,
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
            pets: vec![Pet {
                pet_id: 0,
                unknown2: false,
                unknown3: 0,
                food: 0.0,
                groom: 0.0,
                exercise: 0.0,
                happiness: 0.0,
                unknown8: false,
                pet_trick: vec![PetTrick {
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
                }],
                item_guid: vec![ItemGuid { guid: 0 }],
                battle_class_items: vec![BattleClassItem {
                    item1: 0,
                    item2: Item2 {
                        unknown1: 0,
                        unknown2: 0,
                    },
                }],
                pet_name: "Test".to_string(),
                tint_id: 0,
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
            }],
            pet_unknown1: -1,
            pet_unknown2: 0,
            mounts: owned_mounts,
            action_bars: vec![ActionBar {
                action_bar_type: ActionBarType::Consumable,
                unknown2: 2,
                slots: vec![
                    ActionBarSlot {
                        is_empty: true,
                        icon_id: 0,
                        icon_tint_id: 0,
                        name_id: 0,
                        ability_type: 0,
                        ability_sub_type: 0,
                        unknown7: 0,
                        target_ring_color: 0,
                        required_force_points: 0,
                        is_enabled: false,
                        use_cooldown_millis: 0,
                        init_cooldown_millis: 0,
                        unknown13: 0,
                        quantity: 0,
                        is_consumable: true,
                        millis_since_last_use: 0,
                    },
                    ActionBarSlot {
                        is_empty: true,
                        icon_id: 0,
                        icon_tint_id: 0,
                        name_id: 0,
                        ability_type: 0,
                        ability_sub_type: 0,
                        unknown7: 0,
                        target_ring_color: 0,
                        required_force_points: 0,
                        is_enabled: false,
                        use_cooldown_millis: 0,
                        init_cooldown_millis: 0,
                        unknown13: 0,
                        quantity: 0,
                        is_consumable: true,
                        millis_since_last_use: 0,
                    },
                    ActionBarSlot {
                        is_empty: true,
                        icon_id: 0,
                        icon_tint_id: 0,
                        name_id: 0,
                        ability_type: 0,
                        ability_sub_type: 0,
                        unknown7: 0,
                        target_ring_color: 0,
                        required_force_points: 0,
                        is_enabled: false,
                        use_cooldown_millis: 0,
                        init_cooldown_millis: 0,
                        unknown13: 0,
                        quantity: 0,
                        is_consumable: true,
                        millis_since_last_use: 0,
                    },
                    ActionBarSlot {
                        is_empty: true,
                        icon_id: 0,
                        icon_tint_id: 0,
                        name_id: 0,
                        ability_type: 0,
                        ability_sub_type: 0,
                        unknown7: 0,
                        target_ring_color: 0,
                        required_force_points: 0,
                        is_enabled: false,
                        use_cooldown_millis: 0,
                        init_cooldown_millis: 0,
                        unknown13: 0,
                        quantity: 0,
                        is_consumable: true,
                        millis_since_last_use: 0,
                    },
                ],
            }],
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
        },
    }
}

pub fn make_test_customizations() -> BTreeMap<CustomizationSlot, u32> {
    let mut customizations = BTreeMap::new();
    customizations.insert(CustomizationSlot::HeadModel, 110000);
    customizations.insert(CustomizationSlot::SkinTone, 120030);
    customizations.insert(CustomizationSlot::HairStyle, 130034);
    customizations.insert(CustomizationSlot::HairColor, 140004);
    customizations.insert(CustomizationSlot::EyeColor, 150013);
    customizations.insert(CustomizationSlot::FacialHair, 160004);
    customizations.insert(CustomizationSlot::FacePattern, 170009);
    customizations.insert(CustomizationSlot::BodyModel, 180000);
    customizations
}

pub fn make_test_nameplate_image(guid: u32) -> Vec<Vec<u8>> {
    vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: NameplateImageId {
            image_id: NameplateImage::Trooper,
            guid: player_guid(guid),
        },
    })]
}

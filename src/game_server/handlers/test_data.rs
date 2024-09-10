use std::collections::BTreeMap;

use packet_serialize::{LengthlessVec, SerializePacketError};

use crate::game_server::packets::{
    item::{BaseAttachmentGroup, EquipmentSlot, Item, ItemDefinition, MarketData, WieldType},
    player_data::{
        Ability, ActionBar, BattleClass, BattleClassItem, BattleClassUnknown10, EquippedItem,
        InventoryItem, Item2, ItemGuid, Mount, Pet, PetTrick, Player, PlayerData, Slot, Unknown12,
        Unknown13, Unknown2,
    },
    player_update::{AddNpc, Icon, NameplateImage, NameplateImageId},
    tunnel::TunneledPacket,
    GamePacket, Pos,
};

use super::{
    guid::Guid,
    mount::MountConfig,
    unique_guid::{mount_guid, player_guid},
};

pub fn make_test_npc() -> AddNpc {
    AddNpc {
        guid: 102,
        name_id: 0,
        model_id: 458,
        unknown3: false,
        unknown4: 408679,
        unknown5: 13951728,
        unknown6: 1,
        scale: 1.0,
        pos: Pos {
            x: 887.3,
            y: 171.93376,
            z: 1546.956,
            w: 1.0,
        },
        rot: Pos {
            x: 0.0,
            y: 0.0,
            z: 1.0,
            w: 0.0,
        },
        unknown8: 1,
        attachments: vec![],
        is_not_targetable: 1,
        unknown10: 0,
        texture_name: "Rose".to_string(),
        tint_name: "".to_string(),
        tint_id: 0,
        unknown11: true,
        offset_y: 0.0, // Only enabled when unknown45 == 2
        composite_effect: 0,
        wield_type: WieldType::None,
        name_override: "".to_string(),
        hide_name: true,
        name_offset_x: 0.0,
        name_offset_y: 0.0,
        name_offset_z: 0.0,
        terrain_object_id: 0,
        invisible: false,
        unknown20: 0.0,
        unknown21: false,
        interactable_size_pct: 100,
        unknown23: -1,
        unknown24: -1,
        active_animation_slot: -1,
        unknown26: true,
        ignore_position: true,
        sub_title_id: 0,
        active_animation_slot2: 0,
        head_model_id: 0,
        effects: vec![],
        disable_interact_popup: false,
        unknown33: 0, // If non-zero, crashes when NPC is clicked on
        unknown34: false,
        show_health: false,
        hide_despawn_fade: false,
        ignore_rotation_and_shadow: true,
        base_attachment_group: BaseAttachmentGroup {
            unknown1: 0,
            unknown2: "".to_string(),
            unknown3: "".to_string(),
            unknown4: 0,
            unknown5: "".to_string(),
        },
        unknown39: Pos {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
        unknown40: 0,
        unknown41: -1,
        unknown42: 0,
        collision: true, // To be interactable, every NPC must have collision set,
        // even if the model does not actually support collision
        unknown44: 0,
        npc_type: 0,
        unknown46: 0.0,
        target: 0,
        unknown50: vec![],
        rail_id: 0,
        rail_speed: 0.0,
        rail_origin: Pos {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
        unknown54: 0,
        rail_unknown1: 0.0,
        rail_unknown2: 0.0,
        rail_unknown3: 0.0,
        attachment_group_unknown: "".to_string(),
        unknown59: "".to_string(),
        unknown60: "".to_string(),
        override_terrain_model: false,
        hover_glow: 0,
        hover_description: 0, // max 7
        fly_over_effect: 0,   // max 3
        unknown65: 0,         // max 32
        unknown66: 0,
        unknown67: 0,
        disable_move_to_interact: false,
        unknown69: 0.0,
        unknown70: 0.0,
        unknown71: 0,
        icon_id: Icon::None,
    }
}

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
            guid: mount_guid(guid, mount.guid()),
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

pub fn make_test_customizations() -> Vec<u32> {
    vec![1, 2, 3]
}

pub fn make_test_nameplate_image(guid: u32) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    Ok(vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: NameplateImageId {
            image_id: NameplateImage::Trooper,
            guid: player_guid(guid),
        },
    })?])
}

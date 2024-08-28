use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Error},
    path::Path,
};

use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::DeserializePacket;
use parking_lot::RwLockWriteGuard;
use serde::Deserialize;

use crate::game_server::{
    packets::{
        client_update::{EquipItem, UnequipItem},
        inventory::{EquipGuid, InventoryOpCode, UnequipSlot},
        item::{Attachment, EquipmentSlot, ItemDefinition, WieldType},
        player_data::EquippedItem,
        player_update::UpdateWieldType,
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, GameServer, ProcessPacketError,
};

use super::{
    character::{Character, CharacterType},
    lock_enforcer::CharacterLockRequest,
    unique_guid::player_guid,
};

#[derive(Deserialize)]
pub struct DefaultSaber {
    hilt_item_guid: u32,
    shape_item_guid: u32,
    color_item_guid: u32,
}

pub fn load_default_sabers(config_dir: &Path) -> Result<BTreeMap<u32, DefaultSaber>, Error> {
    let mut file = File::open(config_dir.join("default_sabers.json"))?;
    let default_sabers: Vec<DefaultSaber> = serde_json::from_reader(&mut file)?;
    Ok(default_sabers
        .into_iter()
        .map(|saber| (saber.hilt_item_guid, saber))
        .collect())
}

pub fn process_inventory_packet(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match InventoryOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            InventoryOpCode::UnequipSlot => {
                let unequip_slot: UnequipSlot = DeserializePacket::deserialize(cursor)?;
                game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
                    read_guids: vec![],
                    write_guids: vec![player_guid(sender)],
                    character_consumer: |_, _, mut characters_write, _| {
                        if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {

                            let mut wield_type = character_write_handle.wield_type();
                            let packets = if let CharacterType::Player(ref mut player_data) = character_write_handle.character_type {
                                let possible_battle_class = player_data.battle_classes.get_mut(&unequip_slot.battle_class);

                                if let Some(battle_class) = possible_battle_class {

                                    let mut packets = vec![
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: UnequipItem {
                                                slot: unequip_slot.slot,
                                                battle_class: unequip_slot.battle_class
                                            }
                                        })?
                                    ];

                                    if unequip_slot.slot.is_weapon() {
                                        let is_primary_equipped = battle_class.items.contains_key(&EquipmentSlot::PrimaryWeapon);
                                        wield_type =  match (unequip_slot.slot, wield_type_from_slot(&battle_class.items, unequip_slot.slot, game_server), is_primary_equipped) {
                                            (EquipmentSlot::SecondaryWeapon, WieldType::SingleSaber, true) => WieldType::SingleSaber,
                                            (EquipmentSlot::SecondaryWeapon, WieldType::SinglePistol, true) => WieldType::SinglePistol,
                                            _ => WieldType::None,
                                        };

                                        packets.push(GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: UpdateWieldType {
                                                guid: player_guid(sender),
                                                wield_type,
                                            }
                                        })?);
                                    }

                                    battle_class.items.remove(&unequip_slot.slot);

                                    Ok(vec![Broadcast::Single(sender, packets)])
                                } else {
                                    println!("Player {} tried to unequip slot in battle class {} that they don't own", sender, unequip_slot.battle_class);
                                    Err(ProcessPacketError::CorruptedPacket)
                                }

                            } else {
                                println!("Non-player character {} tried to unequip slot", sender);
                                Err(ProcessPacketError::CorruptedPacket)
                            };

                            character_write_handle.set_wield_type(wield_type);
                            packets

                        } else {
                            println!("Unknown player {} tried to unequip slot", sender);
                            Err(ProcessPacketError::CorruptedPacket)
                        }
                    }
                })
            }
            InventoryOpCode::EquipGuid => {
                let equip_guid: EquipGuid = DeserializePacket::deserialize(cursor)?;
                game_server
                    .lock_enforcer()
                    .read_characters(|_| CharacterLockRequest {
                        read_guids: vec![],
                        write_guids: vec![player_guid(sender)],
                        character_consumer: |_, _, mut characters_write, _| {
                            equip_item_in_slot(
                                sender,
                                &equip_guid,
                                &mut characters_write,
                                game_server,
                                None,
                            )
                            .and_then(|(mut broadcasts, _)| {
                                if equip_guid.slot.is_weapon()
                                    || equip_guid.slot == EquipmentSlot::PrimarySaberColor
                                    || equip_guid.slot == EquipmentSlot::SecondarySaberColor
                                    || equip_guid.slot == EquipmentSlot::PrimarySaberShape
                                    || equip_guid.slot == EquipmentSlot::SecondarySaberShape
                                {
                                    if let Some(character_write_handle) =
                                        characters_write.get(&player_guid(sender))
                                    {
                                        if let CharacterType::Player(player) =
                                            &character_write_handle.character_type
                                        {
                                            if let Some(battle_class) = player
                                                .battle_classes
                                                .get(&player.active_battle_class)
                                            {
                                                broadcasts.append(&mut update_saber_tints(
                                                    sender,
                                                    &battle_class.items,
                                                    player.active_battle_class,
                                                    game_server,
                                                )?);
                                            }
                                        }
                                    }
                                }

                                Ok(broadcasts)
                            })
                        },
                    })
            }
            InventoryOpCode::EquipSaber => {
                let equip_guid: EquipGuid = DeserializePacket::deserialize(cursor)?;
                let (shape_slot, color_slot) = match &equip_guid.slot {
                    EquipmentSlot::PrimaryWeapon => (
                        EquipmentSlot::PrimarySaberShape,
                        EquipmentSlot::PrimarySaberColor,
                    ),
                    EquipmentSlot::SecondaryWeapon => (
                        EquipmentSlot::SecondarySaberShape,
                        EquipmentSlot::SecondarySaberColor,
                    ),
                    _ => {
                        println!(
                            "Player {} tried to equip saber in slot {:?}",
                            sender, equip_guid.slot
                        );
                        return Err(ProcessPacketError::CorruptedPacket);
                    }
                };

                game_server
                    .lock_enforcer()
                    .read_characters(|_| CharacterLockRequest {
                        read_guids: vec![],
                        write_guids: vec![player_guid(sender)],
                        character_consumer: |_, _, mut characters_write, _| {
                            let mut broadcasts = Vec::new();
                            if let Some(saber) =
                                game_server.default_sabers().get(&equip_guid.item_guid)
                            {
                                broadcasts.append(
                                    &mut equip_item_in_slot(
                                        sender,
                                        &equip_guid,
                                        &mut characters_write,
                                        game_server,
                                        None,
                                    )?
                                    .0,
                                );

                                let (mut color_broadcasts, tint) = equip_item_in_slot(
                                    sender,
                                    &EquipGuid {
                                        item_guid: saber.color_item_guid,
                                        battle_class: equip_guid.battle_class,
                                        slot: color_slot,
                                    },
                                    &mut characters_write,
                                    game_server,
                                    None,
                                )?;
                                broadcasts.append(&mut color_broadcasts);

                                broadcasts.append(
                                    &mut equip_item_in_slot(
                                        sender,
                                        &EquipGuid {
                                            item_guid: saber.shape_item_guid,
                                            battle_class: equip_guid.battle_class,
                                            slot: shape_slot,
                                        },
                                        &mut characters_write,
                                        game_server,
                                        Some(tint),
                                    )?
                                    .0,
                                );
                            } else {
                                println!(
                                    "Player {} tried to equip unknown saber {}",
                                    sender, equip_guid.item_guid
                                );
                                return Err(ProcessPacketError::CorruptedPacket);
                            }
                            Ok(broadcasts)
                        },
                    })
            }
            _ => {
                println!(
                    "Unimplemented inventory packet: {:?}, {:x?}",
                    op_code, cursor
                );
                Ok(Vec::new())
            }
        },
        Err(_) => {
            println!("Unknown inventory packet: {}, {:x?}", raw_op_code, cursor);
            Ok(Vec::new())
        }
    }
}

pub fn wield_type_from_slot(
    items: &BTreeMap<EquipmentSlot, EquippedItem>,
    slot: EquipmentSlot,
    game_server: &GameServer,
) -> WieldType {
    item_def_from_slot(items, slot, game_server)
        .and_then(|item_def| {
            game_server
                .item_classes()
                .definitions
                .get(&item_def.item_class)
        })
        .map(|item_class| item_class.wield_type)
        .unwrap_or(WieldType::None)
}

fn item_def_from_slot<'a>(
    items: &BTreeMap<EquipmentSlot, EquippedItem>,
    slot: EquipmentSlot,
    game_server: &'a GameServer,
) -> Option<&'a ItemDefinition> {
    items
        .get(&slot)
        .and_then(|item_guid| game_server.items().get(&item_guid.guid))
}

pub fn update_saber_tints(
    sender: u32,
    items: &BTreeMap<EquipmentSlot, EquippedItem>,
    battle_class: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let mut packets = Vec::new();

    if let Some(primary_shape_def) =
        item_def_from_slot(items, EquipmentSlot::PrimarySaberShape, game_server)
    {
        if let Some(primary_color_def) =
            item_def_from_slot(items, EquipmentSlot::PrimarySaberColor, game_server)
        {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: EquipItem {
                    item_guid: primary_shape_def.guid,
                    attachment: Attachment {
                        model_name: primary_shape_def.model_name.clone(),
                        texture_alias: primary_shape_def.texture_alias.clone(),
                        tint_alias: primary_shape_def.tint_alias.clone(),
                        tint: primary_color_def.tint,
                        composite_effect: primary_shape_def.composite_effect,
                        slot: EquipmentSlot::PrimarySaberShape,
                    },
                    battle_class,
                    item_class: primary_shape_def.item_class,
                    equip: true,
                },
            })?);
        }
    }

    if let Some(secondary_shape_def) =
        item_def_from_slot(items, EquipmentSlot::SecondarySaberShape, game_server)
    {
        if let Some(secondary_color_def) =
            item_def_from_slot(items, EquipmentSlot::SecondarySaberColor, game_server)
        {
            packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: EquipItem {
                    item_guid: secondary_shape_def.guid,
                    attachment: Attachment {
                        model_name: secondary_shape_def.model_name.clone(),
                        texture_alias: secondary_shape_def.texture_alias.clone(),
                        tint_alias: secondary_shape_def.tint_alias.clone(),
                        tint: secondary_color_def.tint,
                        composite_effect: secondary_shape_def.composite_effect,
                        slot: EquipmentSlot::SecondarySaberShape,
                    },
                    battle_class,
                    item_class: secondary_shape_def.item_class,
                    equip: true,
                },
            })?);
        }
    }

    Ok(vec![Broadcast::Single(sender, packets)])
}

fn equip_item_in_slot(
    sender: u32,
    equip_guid: &EquipGuid,
    characters_write: &mut BTreeMap<u64, RwLockWriteGuard<Character>>,
    game_server: &GameServer,
    tint_override: Option<u32>,
) -> Result<(Vec<Broadcast>, u32), ProcessPacketError> {
    if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {
        let mut wield_type = character_write_handle.wield_type();
        let broadcasts = if let CharacterType::Player(ref mut player_data) =
            character_write_handle.character_type
        {
            if player_data.inventory.contains(&equip_guid.item_guid) {
                let possible_battle_class =
                    player_data.battle_classes.get_mut(&equip_guid.battle_class);

                if let Some(battle_class) = possible_battle_class {
                    if let Some(item_def) = game_server.items().get(&equip_guid.item_guid) {
                        let mut packets = vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: EquipItem {
                                item_guid: equip_guid.item_guid,
                                attachment: Attachment {
                                    model_name: item_def.model_name.clone(),
                                    texture_alias: item_def.texture_alias.clone(),
                                    tint_alias: item_def.tint_alias.clone(),
                                    tint: tint_override.unwrap_or(item_def.tint),
                                    composite_effect: item_def.composite_effect,
                                    slot: equip_guid.slot,
                                },
                                battle_class: equip_guid.battle_class,
                                item_class: item_def.item_class,
                                equip: true,
                            },
                        })?];

                        if let Some(item_class) = game_server
                            .item_classes()
                            .definitions
                            .get(&item_def.item_class)
                        {
                            if equip_guid.slot.is_weapon() {
                                if equip_guid.slot == EquipmentSlot::PrimaryWeapon {
                                    let secondary_wield_type = wield_type_from_slot(
                                        &battle_class.items,
                                        EquipmentSlot::SecondaryWeapon,
                                        game_server,
                                    );
                                    if item_class.wield_type != secondary_wield_type {
                                        packets.push(GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: UnequipItem {
                                                slot: EquipmentSlot::SecondaryWeapon,
                                                battle_class: equip_guid.battle_class,
                                            },
                                        })?);
                                        battle_class.items.remove(&EquipmentSlot::SecondaryWeapon);
                                    }
                                }

                                let is_secondary_equipped = battle_class
                                    .items
                                    .contains_key(&EquipmentSlot::SecondaryWeapon);
                                wield_type = match (
                                    equip_guid.slot,
                                    item_class.wield_type,
                                    is_secondary_equipped,
                                ) {
                                    (
                                        EquipmentSlot::PrimaryWeapon,
                                        WieldType::SingleSaber,
                                        false,
                                    ) => WieldType::SingleSaber,
                                    (
                                        EquipmentSlot::PrimaryWeapon,
                                        WieldType::SinglePistol,
                                        false,
                                    ) => WieldType::SinglePistol,
                                    (
                                        EquipmentSlot::PrimaryWeapon,
                                        WieldType::SingleSaber,
                                        true,
                                    ) => WieldType::DualSaber,
                                    (
                                        EquipmentSlot::PrimaryWeapon,
                                        WieldType::SinglePistol,
                                        true,
                                    ) => WieldType::DualPistol,
                                    (EquipmentSlot::SecondaryWeapon, WieldType::SingleSaber, _) => {
                                        WieldType::DualSaber
                                    }
                                    (
                                        EquipmentSlot::SecondaryWeapon,
                                        WieldType::SinglePistol,
                                        _,
                                    ) => WieldType::DualPistol,
                                    _ => item_class.wield_type,
                                };

                                packets.push(GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: UpdateWieldType {
                                        guid: player_guid(sender),
                                        wield_type,
                                    },
                                })?);
                            }
                        }

                        battle_class.items.insert(
                            equip_guid.slot,
                            EquippedItem {
                                slot: equip_guid.slot,
                                guid: equip_guid.item_guid,
                                category: item_def.category,
                            },
                        );

                        Ok((vec![Broadcast::Single(sender, packets)], item_def.tint))
                    } else {
                        println!(
                            "Player {} tried to equip unknown item {}",
                            sender, equip_guid.item_guid
                        );
                        Err(ProcessPacketError::CorruptedPacket)
                    }
                } else {
                    println!(
                        "Player {} tried to equip item in battle class {} that they don't own",
                        sender, equip_guid.battle_class
                    );
                    Err(ProcessPacketError::CorruptedPacket)
                }
            } else {
                println!(
                    "Player {} tried to equip item {} that they don't own",
                    sender, equip_guid.battle_class
                );
                Err(ProcessPacketError::CorruptedPacket)
            }
        } else {
            println!("Non-player character {} tried to equip item", sender);
            Err(ProcessPacketError::CorruptedPacket)
        };

        character_write_handle.set_wield_type(wield_type);
        broadcasts
    } else {
        println!("Unknown player {} tried to equip item", sender);
        Err(ProcessPacketError::CorruptedPacket)
    }
}

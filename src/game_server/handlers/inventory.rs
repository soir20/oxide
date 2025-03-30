use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Error},
    iter,
    path::Path,
};

use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::DeserializePacket;
use parking_lot::RwLockWriteGuard;
use serde::Deserialize;

use crate::{
    game_server::{
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
        packets::{
            GamePacket,
            client_update::{EquipItem, UnequipItem, UpdateCredits},
            inventory::{
                EquipCustomization, EquipGuid, InventoryOpCode, PreviewCustomization, UnequipSlot,
            },
            item::{Attachment, EquipmentSlot, ItemDefinition, WieldType},
            player_data::EquippedItem,
            player_update::{
                Customization, UpdateCustomizations, UpdateEquippedItem, UpdateWieldType,
            },
            tunnel::TunneledPacket,
            ui::ExecuteScriptWithParams,
        },
    },
    info,
};

use super::{
    character::{
        Character, CharacterLocationIndex, CharacterMatchmakingGroupIndex, CharacterNameIndex,
        CharacterSquadIndex, CharacterType, Chunk,
    },
    guid::{GuidTableIndexer, IndexedGuid},
    lock_enforcer::CharacterLockRequest,
    unique_guid::player_guid,
    zone::ZoneInstance,
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

pub fn load_customizations(config_dir: &Path) -> Result<BTreeMap<u32, Customization>, Error> {
    let mut file = File::open(config_dir.join("customizations.json"))?;
    let customizations: Vec<Customization> = serde_json::from_reader(&mut file)?;
    Ok(customizations
        .into_iter()
        .map(|customization: Customization| (customization.guid, customization))
        .collect())
}

pub fn load_customization_item_mappings(
    config_dir: &Path,
) -> Result<BTreeMap<u32, Vec<u32>>, Error> {
    let mut file = File::open(config_dir.join("customization_item_mappings.json"))?;
    Ok(serde_json::from_reader(&mut file)?)
}

pub fn process_inventory_packet(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match InventoryOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            InventoryOpCode::UnequipSlot => process_unequip_slot(game_server, cursor, sender),
            InventoryOpCode::EquipGuid => process_equip_guid(game_server, cursor, sender),
            InventoryOpCode::EquipSaber => process_equip_saber(game_server, cursor, sender),
            InventoryOpCode::PreviewCustomization => {
                process_preview_customization(game_server, cursor, sender)
            }
            InventoryOpCode::EquipCustomization => {
                process_equip_customization(game_server, cursor, sender)
            }
        },
        Err(_) => Err(ProcessPacketError::new(
            ProcessPacketErrorType::UnknownOpCode,
            format!("Unknown inventory packet: {}, {:x?}", raw_op_code, cursor),
        )),
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

pub fn customizations_from_guids(
    applied_customizations: impl Iterator<Item = u32>,
    customizations: &BTreeMap<u32, Customization>,
) -> Vec<Customization> {
    let mut result = Vec::new();

    for customization_guid in applied_customizations {
        if let Some(customization) = customizations.get(&customization_guid) {
            result.push(customization.clone());
        } else {
            info!(
                "Skipped adding unknown customization {}",
                customization_guid
            )
        }
    }

    result
}

pub fn customizations_from_item_guids(
    sender: u32,
    applied_customization_item_guids: impl Iterator<Item = u32>,
    customizations: &BTreeMap<u32, Customization>,
    customization_item_mappings: &BTreeMap<u32, Vec<u32>>,
) -> Result<Vec<Customization>, ProcessPacketError> {
    let mut result = Vec::new();

    for customization_item_guid in applied_customization_item_guids {
        if let Some(customizations_for_item) =
            customization_item_mappings.get(&customization_item_guid)
        {
            for customization_guid in customizations_for_item {
                if let Some(customization) = customizations.get(customization_guid) {
                    result.push(customization.clone());
                } else {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Player {} tried to use unknown customization {}",
                            sender, customization_guid
                        ),
                    ));
                }
            }
        } else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {} tried to use unknown customization item guid {}",
                    sender, customization_item_guid
                ),
            ));
        }
    }

    Ok(result)
}

fn process_unequip_slot(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let unequip_slot: UnequipSlot = DeserializePacket::deserialize(cursor)?;
    game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
            read_guids: vec![],
            write_guids: vec![player_guid(sender)],
            character_consumer: |characters_table_read_handle, _, mut characters_write, _| {
                if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {

                    let mut brandished_wield_type = None;
                    let mut broadcasts = if let CharacterType::Player(ref mut player_data) = character_write_handle.stats.character_type {
                        let possible_battle_class = player_data.battle_classes.get_mut(&unequip_slot.battle_class);

                        if let Some(battle_class) = possible_battle_class {
                            battle_class.items.remove(&unequip_slot.slot);

                            // There are no weapons that allow equipping both weapon slots and then unequipping only the primary slot.
                            // You can only unequip the secondary slot or unequip both slots after you equip both slots. Therefore, after 
                            // an item is unequipped, only the primary slot can influence the wield type.
                            if unequip_slot.slot.is_weapon() {
                                brandished_wield_type = Some(wield_type_from_slot(&battle_class.items, EquipmentSlot::PrimaryWeapon, game_server));
                            }

                            Ok(vec![
                                Broadcast::Single(sender, vec![
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: UnequipItem {
                                            slot: unequip_slot.slot,
                                            battle_class: unequip_slot.battle_class
                                        }
                                    })?
                                ])
                            ])
                        } else {
                            Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} tried to unequip slot in battle class {} that they don't own", sender, unequip_slot.battle_class)))
                        }

                    } else {
                        Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Non-player character {} tried to unequip slot", sender)))
                    }?;

                    let mut all_player_packets = Vec::new();
                    if let Some(wield_type) = brandished_wield_type {
                        character_write_handle.set_brandished_wield_type(wield_type);

                        all_player_packets.push(GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: UpdateWieldType {
                                guid: player_guid(sender),
                                wield_type,
                            }
                        })?);
                    }

                    all_player_packets.push(
                        GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: UpdateEquippedItem {
                                guid: player_guid(sender),
                                item_guid: 0,
                                item: Attachment {
                                    model_name: "".to_string(),
                                    texture_alias: "".to_string(),
                                    tint_alias: "".to_string(),
                                    tint: 0,
                                    composite_effect: 0,
                                    slot: unequip_slot.slot,
                                },
                                battle_class: unequip_slot.battle_class,
                                wield_type: character_write_handle.stats.wield_type()
                            }
                        })?
                    );

                    let (_, instance_guid, chunk) = character_write_handle.index1();
                    let all_players_nearby = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle)?;
                    broadcasts.push(Broadcast::Multi(all_players_nearby, all_player_packets));

                    Ok(broadcasts)

                } else {
                    Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Unknown player {} tried to unequip slot", sender)))
                }
            }
        })
}

fn process_equip_guid(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let equip_guid: EquipGuid = DeserializePacket::deserialize(cursor)?;
    game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: vec![],
            write_guids: vec![player_guid(sender)],
            character_consumer: |characters_table_read_handle, _, mut characters_write, _| {
                equip_item_in_slot(
                    sender,
                    &equip_guid,
                    characters_table_read_handle,
                    &mut characters_write,
                    game_server,
                    None,
                )
                .and_then(|(mut broadcasts, _)| {
                    if equip_guid.slot.is_saber() {
                        if let Some(character_write_handle) =
                            characters_write.get(&player_guid(sender))
                        {
                            if let CharacterType::Player(player) =
                                &character_write_handle.stats.character_type
                            {
                                if let Some(battle_class) =
                                    player.battle_classes.get(&player.active_battle_class)
                                {
                                    let (_, instance_guid, chunk) = character_write_handle.index1();
                                    broadcasts.append(&mut update_saber_tints(
                                        sender,
                                        characters_table_read_handle,
                                        instance_guid,
                                        chunk,
                                        &battle_class.items,
                                        player.active_battle_class,
                                        character_write_handle.stats.wield_type(),
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

fn process_equip_saber(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
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
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {} tried to equip saber in slot {:?}",
                    sender, equip_guid.slot
                ),
            ));
        }
    };

    game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: vec![],
            write_guids: vec![player_guid(sender)],
            character_consumer: |characters_table_read_handle, _, mut characters_write, _| {
                let mut broadcasts = Vec::new();
                if let Some(saber) = game_server.default_sabers().get(&equip_guid.item_guid) {
                    broadcasts.append(
                        &mut equip_item_in_slot(
                            sender,
                            &equip_guid,
                            characters_table_read_handle,
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
                        characters_table_read_handle,
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
                            characters_table_read_handle,
                            &mut characters_write,
                            game_server,
                            Some(tint),
                        )?
                        .0,
                    );
                } else {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Player {} tried to equip unknown saber {}",
                            sender, equip_guid.item_guid
                        ),
                    ));
                }
                Ok(broadcasts)
            },
        })
}

fn process_preview_customization(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let preview_customization: PreviewCustomization = DeserializePacket::deserialize(cursor)?;
    Ok(vec![Broadcast::Single(
        sender,
        vec![GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdateCustomizations {
                guid: player_guid(sender),
                is_preview: true,
                customizations: customizations_from_item_guids(
                    sender,
                    iter::once(preview_customization.item_guid),
                    game_server.customizations(),
                    game_server.customization_item_mappings(),
                )?,
            },
        })?],
    )])
}

fn process_equip_customization(
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let equip_customization: EquipCustomization = DeserializePacket::deserialize(cursor)?;
    game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: vec![],
            write_guids: vec![player_guid(sender)],
            character_consumer: |characters_table_read_handle, _, mut characters_write, _| {
                if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender))
                {
                    if let CharacterType::Player(player) =
                        &mut character_write_handle.stats.character_type
                    {
                        let cost = if let Some(cost_entry) =
                            game_server.costs().get(&equip_customization.item_guid)
                        {
                            if player.member {
                                cost_entry.members
                            } else {
                                cost_entry.base
                            }
                        } else {
                            0
                        };

                        if cost > player.credits {
                            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} tried to purchase customization {} for {} but only has {} credits", sender, equip_customization.item_guid, cost, player.credits)));
                        }
                        player.credits -= cost;
                        let new_credits = player.credits;

                        let customizations_to_apply = customizations_from_item_guids(
                            sender,
                            iter::once(equip_customization.item_guid),
                            game_server.customizations(),
                            game_server.customization_item_mappings(),
                        )?;

                        for customization in &customizations_to_apply {
                            player
                                .customizations
                                .insert(customization.customization_slot, customization.guid);
                        }

                        let (_, instance_guid, chunk) = character_write_handle.index1();
                        let nearby_players = ZoneInstance::all_players_nearby(
                            chunk,
                            instance_guid,
                            characters_table_read_handle,
                        )?;

                        Ok(vec![
                            Broadcast::Multi(
                                nearby_players,
                                vec![GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: UpdateCustomizations {
                                        guid: player_guid(sender),
                                        is_preview: false,
                                        customizations: customizations_to_apply,
                                    },
                                })?],
                            ),
                            Broadcast::Single(
                                sender,
                                vec![
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: UpdateCredits { new_credits },
                                    })?,
                                    // Fix UI not updating the equipped customization item ID properly
                                    GamePacket::serialize(&TunneledPacket {
                                        unknown1: true,
                                        inner: ExecuteScriptWithParams {
                                            script_name:
                                                "CharacterWindowHandler.requestDataSourceUpdate"
                                                    .to_string(),
                                            params: vec!["BaseClient.CustomizationItemDataSource"
                                                .to_string()],
                                        },
                                    })?,
                                ],
                            ),
                        ])
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Non-player character {} tried to equip customization",
                                sender
                            ),
                        ))
                    }
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!("Unknown player {} tried to equip customization", sender),
                    ))
                }
            },
        })
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

pub fn update_saber_tints<'a>(
    sender: u32,
    characters_table_handle: &'a impl GuidTableIndexer<
        'a,
        u64,
        Character,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
    >,
    instance_guid: u64,
    chunk: Chunk,
    items: &BTreeMap<EquipmentSlot, EquippedItem>,
    battle_class: u32,
    wield_type: WieldType,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let mut sender_only_packets = Vec::new();
    let mut nearby_player_packets = Vec::new();

    if let Some(primary_shape_def) =
        item_def_from_slot(items, EquipmentSlot::PrimarySaberShape, game_server)
    {
        if let Some(primary_color_def) =
            item_def_from_slot(items, EquipmentSlot::PrimarySaberColor, game_server)
        {
            sender_only_packets.push(GamePacket::serialize(&TunneledPacket {
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
            nearby_player_packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: UpdateEquippedItem {
                    guid: player_guid(sender),
                    item_guid: primary_shape_def.guid,
                    item: Attachment {
                        model_name: primary_shape_def.model_name.clone(),
                        texture_alias: primary_shape_def.texture_alias.clone(),
                        tint_alias: primary_shape_def.tint_alias.clone(),
                        tint: primary_color_def.tint,
                        composite_effect: primary_shape_def.composite_effect,
                        slot: EquipmentSlot::PrimarySaberShape,
                    },
                    battle_class,
                    wield_type,
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
            sender_only_packets.push(GamePacket::serialize(&TunneledPacket {
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
            nearby_player_packets.push(GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: UpdateEquippedItem {
                    guid: player_guid(sender),
                    item_guid: secondary_shape_def.guid,
                    item: Attachment {
                        model_name: secondary_shape_def.model_name.clone(),
                        texture_alias: secondary_shape_def.texture_alias.clone(),
                        tint_alias: secondary_shape_def.tint_alias.clone(),
                        tint: secondary_color_def.tint,
                        composite_effect: secondary_shape_def.composite_effect,
                        slot: EquipmentSlot::SecondarySaberShape,
                    },
                    battle_class,
                    wield_type,
                },
            })?);
        }
    }

    let other_players_nearby = ZoneInstance::other_players_nearby(
        Some(sender),
        chunk,
        instance_guid,
        characters_table_handle,
    )?;
    Ok(vec![
        Broadcast::Single(sender, sender_only_packets),
        Broadcast::Multi(other_players_nearby, nearby_player_packets),
    ])
}

fn equip_item_in_slot<'a>(
    sender: u32,
    equip_guid: &EquipGuid,
    characters_table_handle: &'a impl GuidTableIndexer<
        'a,
        u64,
        Character,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
    >,
    characters_write: &mut BTreeMap<u64, RwLockWriteGuard<Character>>,
    game_server: &GameServer,
    tint_override: Option<u32>,
) -> Result<(Vec<Broadcast>, u32), ProcessPacketError> {
    if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {
        let current_wield_type = character_write_handle.stats.wield_type();

        // Always brandish a saber when any saber component changes. If we're equipping a new saber, the
        // wield type will be updated appropriately later.
        let mut brandished_wield_type = if equip_guid.slot.is_saber() {
            Some(character_write_handle.brandished_wield_type())
        } else {
            None
        };

        let (sender_only_packets, other_player_packets, tint) = if let CharacterType::Player(
            ref mut player_data,
        ) =
            character_write_handle.stats.character_type
        {
            if player_data.inventory.contains(&equip_guid.item_guid) {
                let possible_battle_class =
                    player_data.battle_classes.get_mut(&equip_guid.battle_class);

                if let Some(battle_class) = possible_battle_class {
                    if equip_guid.slot == EquipmentSlot::SecondaryWeapon
                        && !battle_class
                            .items
                            .contains_key(&EquipmentSlot::PrimaryWeapon)
                    {
                        return Ok((Vec::new(), 0));
                    }

                    if let Some(item_def) = game_server.items().get(&equip_guid.item_guid) {
                        let mut sender_only_packets =
                            vec![GamePacket::serialize(&TunneledPacket {
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
                        let mut all_player_packets =
                            vec![GamePacket::serialize(&TunneledPacket {
                                unknown1: true,
                                inner: UpdateEquippedItem {
                                    guid: player_guid(sender),
                                    item_guid: equip_guid.item_guid,
                                    item: Attachment {
                                        model_name: item_def.model_name.clone(),
                                        texture_alias: item_def.texture_alias.clone(),
                                        tint_alias: item_def.tint_alias.clone(),
                                        tint: tint_override.unwrap_or(item_def.tint),
                                        composite_effect: item_def.composite_effect,
                                        slot: equip_guid.slot,
                                    },
                                    battle_class: equip_guid.battle_class,
                                    wield_type: current_wield_type,
                                },
                            })?];

                        if let Some(item_class) = game_server
                            .item_classes()
                            .definitions
                            .get(&item_def.item_class)
                        {
                            if equip_guid.slot.is_weapon() {
                                // Some weapons, like bows, can be equipped in the secondary slot without
                                // a primary weapon, so check the opposite slot instead of the primary slot.
                                let other_weapon_slot = other_weapon_slot(equip_guid.slot);
                                let other_wield_type = wield_type_from_slot(
                                    &battle_class.items,
                                    other_weapon_slot,
                                    game_server,
                                );
                                if item_class.wield_type != other_wield_type {
                                    sender_only_packets.push(GamePacket::serialize(
                                        &TunneledPacket {
                                            unknown1: true,
                                            inner: UnequipItem {
                                                slot: other_weapon_slot,
                                                battle_class: equip_guid.battle_class,
                                            },
                                        },
                                    )?);
                                    all_player_packets.push(GamePacket::serialize(
                                        &TunneledPacket {
                                            unknown1: true,
                                            inner: UpdateEquippedItem {
                                                guid: player_guid(sender),
                                                item_guid: 0,
                                                item: Attachment {
                                                    model_name: "".to_string(),
                                                    texture_alias: "".to_string(),
                                                    tint_alias: "".to_string(),
                                                    tint: 0,
                                                    composite_effect: 0,
                                                    slot: other_weapon_slot,
                                                },
                                                battle_class: equip_guid.battle_class,
                                                wield_type: current_wield_type,
                                            },
                                        },
                                    )?);
                                    battle_class.items.remove(&other_weapon_slot);
                                }

                                let is_secondary_equipped = battle_class
                                    .items
                                    .contains_key(&EquipmentSlot::SecondaryWeapon);
                                let wield_type = match (
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
                                brandished_wield_type = Some(wield_type);
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

                        Ok((sender_only_packets, all_player_packets, item_def.tint))
                    } else {
                        Err(ProcessPacketError::new(
                            ProcessPacketErrorType::ConstraintViolated,
                            format!(
                                "Player {} tried to equip unknown item {}",
                                sender, equip_guid.item_guid
                            ),
                        ))
                    }
                } else {
                    Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Player {} tried to equip item in battle class {} that they don't own",
                            sender, equip_guid.battle_class
                        ),
                    ))
                }
            } else {
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {} tried to equip item {} that they don't own",
                        sender, equip_guid.battle_class
                    ),
                ))
            }
        } else {
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Non-player character {} tried to equip item", sender),
            ))
        }?;

        let (_, instance_guid, chunk) = character_write_handle.index1();
        let mut nearby_players = ZoneInstance::other_players_nearby(
            Some(sender),
            chunk,
            instance_guid,
            characters_table_handle,
        )?;
        let mut broadcasts = vec![
            Broadcast::Single(sender, sender_only_packets),
            Broadcast::Multi(nearby_players.clone(), other_player_packets),
        ];

        if let Some(wield_type) = brandished_wield_type {
            character_write_handle.set_brandished_wield_type(wield_type);

            nearby_players.push(sender);
            broadcasts.push(Broadcast::Multi(
                nearby_players,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: UpdateWieldType {
                        guid: player_guid(sender),
                        wield_type,
                    },
                })?],
            ));
        }

        Ok((broadcasts, tint))
    } else {
        Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!("Unknown player {} tried to equip item", sender),
        ))
    }
}

fn other_weapon_slot(slot: EquipmentSlot) -> EquipmentSlot {
    match slot {
        EquipmentSlot::PrimaryWeapon => EquipmentSlot::SecondaryWeapon,
        EquipmentSlot::SecondaryWeapon => EquipmentSlot::PrimaryWeapon,
        _ => EquipmentSlot::None,
    }
}

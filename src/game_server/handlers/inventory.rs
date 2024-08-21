use std::{collections::BTreeMap, io::Cursor};

use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::DeserializePacket;

use crate::game_server::{
    packets::{
        client_update::{EquipItem, UnequipItem},
        inventory::{EquipGuid, InventoryOpCode, UnequipSlot},
        item::{Attachment, EquipmentSlot, WieldType},
        player_data::EquippedItem,
        player_update::UpdateWieldType,
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, GameServer, ProcessPacketError,
};

use super::{
    character::CharacterType, lock_enforcer::CharacterLockRequest, unique_guid::player_guid,
};

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

                            let mut wield_type = character_write_handle.wield_type;
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

                            character_write_handle.wield_type = wield_type;
                            packets

                        } else {
                            println!("Unknown player {} tried to unequip slot", sender);
                            Err(ProcessPacketError::CorruptedPacket)
                        }
                    }
                })
            }
            InventoryOpCode::EquipGuid | InventoryOpCode::EquipSaber => {
                let equip_guid: EquipGuid = DeserializePacket::deserialize(cursor)?;
                game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
                    read_guids: vec![],
                    write_guids: vec![player_guid(sender)],
                    character_consumer: |_, _, mut characters_write, _| {
                        if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {

                            let mut wield_type = character_write_handle.wield_type;
                            let packets = if let CharacterType::Player(ref mut player_data) = character_write_handle.character_type {

                                if player_data.inventory.contains(&equip_guid.item_guid) {
                                    let possible_battle_class = player_data.battle_classes.get_mut(&equip_guid.battle_class);

                                    if let Some(battle_class) = possible_battle_class {

                                        if let Some(item_def) = game_server.items().get(&equip_guid.item_guid) {
                                            let mut packets = vec![
                                                GamePacket::serialize(&TunneledPacket {
                                                    unknown1: true,
                                                    inner: EquipItem {
                                                        item_guid: equip_guid.item_guid,
                                                        attachment: Attachment {
                                                            model_name: item_def.model_name.clone(),
                                                            texture_alias: item_def.texture_alias.clone(),
                                                            tint_alias: item_def.tint_alias.clone(),
                                                            tint: item_def.tint,
                                                            composite_effect: item_def.composite_effect,
                                                            slot: equip_guid.slot
                                                        },
                                                        battle_class: equip_guid.battle_class,
                                                        item_class: item_def.item_class,
                                                        equip: true,
                                                    }
                                                })?
                                            ];

                                            if let Some(item_class) = game_server.item_classes().definitions.get(&item_def.item_class) {
                                                if equip_guid.slot.is_weapon() {

                                                    if equip_guid.slot == EquipmentSlot::PrimaryWeapon {
                                                        let secondary_wield_type = wield_type_from_slot(&battle_class.items, EquipmentSlot::SecondaryWeapon, game_server);
                                                        if item_class.wield_type != secondary_wield_type {
                                                            packets.push(GamePacket::serialize(&TunneledPacket {
                                                                unknown1: true,
                                                                inner: UnequipItem {
                                                                    slot: EquipmentSlot::SecondaryWeapon,
                                                                    battle_class: equip_guid.battle_class
                                                                }
                                                            })?);
                                                            battle_class.items.remove(&EquipmentSlot::SecondaryWeapon);
                                                        }
                                                    }

                                                    let is_secondary_equipped = battle_class.items.contains_key(&EquipmentSlot::SecondaryWeapon);
                                                    wield_type = match (equip_guid.slot, item_class.wield_type, is_secondary_equipped) {
                                                        (EquipmentSlot::PrimaryWeapon, WieldType::SingleSaber, false) => WieldType::SingleSaber,
                                                        (EquipmentSlot::PrimaryWeapon, WieldType::SinglePistol, false) => WieldType::SinglePistol,
                                                        (EquipmentSlot::PrimaryWeapon, WieldType::SingleSaber, true) => WieldType::DualSaber,
                                                        (EquipmentSlot::PrimaryWeapon, WieldType::SinglePistol, true) => WieldType::DualPistol,
                                                        (EquipmentSlot::SecondaryWeapon, WieldType::SingleSaber, _) => WieldType::DualSaber,
                                                        (EquipmentSlot::SecondaryWeapon, WieldType::SinglePistol, _) => WieldType::DualPistol,
                                                        _ => item_class.wield_type,
                                                    };

                                                    packets.push(GamePacket::serialize(&TunneledPacket {
                                                        unknown1: true,
                                                        inner: UpdateWieldType {
                                                            guid: player_guid(sender),
                                                            wield_type,
                                                        }
                                                    })?);
                                                }
                                            }

                                            battle_class.items.insert(equip_guid.slot, EquippedItem {
                                                slot: equip_guid.slot,
                                                guid: equip_guid.item_guid,
                                                category: item_def.category,
                                            });

                                            Ok(vec![Broadcast::Single(sender, packets)])
                                        } else {
                                            println!("Player {} tried to equip unknown item {}", sender, equip_guid.item_guid);
                                            Err(ProcessPacketError::CorruptedPacket)
                                        }

                                    } else {
                                        println!("Player {} tried to equip item in battle class {} that they don't own", sender, equip_guid.battle_class);
                                        Err(ProcessPacketError::CorruptedPacket)
                                    }

                                } else {
                                    println!("Player {} tried to equip item {} that they don't own", sender, equip_guid.battle_class);
                                        Err(ProcessPacketError::CorruptedPacket)
                                }

                            } else {
                                println!("Non-player character {} tried to equip item", sender);
                                Err(ProcessPacketError::CorruptedPacket)
                            };

                            character_write_handle.wield_type = wield_type;
                            packets

                        } else {
                            println!("Unknown player {} tried to equip item", sender);
                            Err(ProcessPacketError::CorruptedPacket)
                        }
                    }
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
    items
        .get(&slot)
        .and_then(|item| game_server.items().get(&item.guid))
        .and_then(|item_def| {
            game_server
                .item_classes()
                .definitions
                .get(&item_def.item_class)
        })
        .map(|item_class| item_class.wield_type)
        .unwrap_or(WieldType::None)
}

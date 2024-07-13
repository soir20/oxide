use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{
    client_update_packet::UnequipItem,
    game_packet::{GamePacket, OpCode},
    item::EquipmentSlot,
    lock_enforcer::CharacterLockRequest,
    player_data,
    player_update_packet::{Attachment, UpdateEquippedItem},
    tunnel::TunneledPacket,
    unique_guid::player_guid,
    zone::CharacterType,
    Broadcast, GameServer, ProcessPacketError,
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
                            if let CharacterType::Player(ref mut player_data) = character_write_handle.character_type {
                                let possible_battle_class = player_data.battle_classes.iter_mut()
                                    .find(|battle_class| battle_class.guid == unequip_slot.battle_class);
                                if let Some(battle_class) = possible_battle_class {
                                    battle_class.items.retain(|equipped_item| equipped_item.slot != unequip_slot.slot);
                                    Ok(vec![Broadcast::Single(sender, vec![
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: UnequipItem {
                                                slot: unequip_slot.slot,
                                                battle_class_id: unequip_slot.battle_class
                                            }
                                        })?
                                    ])])
                                } else {
                                    println!("Player {} tried to unequip slot in battle class {} that they don't own", sender, unequip_slot.battle_class);
                                    Err(ProcessPacketError::CorruptedPacket)
                                }
                            } else {
                                println!("Non-player character {} tried to unequip slot", sender);
                                Err(ProcessPacketError::CorruptedPacket)
                            }
                        } else {
                            println!("Unknown player {} tried to unequip slot", sender);
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

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum InventoryOpCode {
    UnequipSlot = 0x2,
}

impl SerializePacket for InventoryOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Inventory.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(DeserializePacket)]
pub struct UnequipSlot {
    slot: EquipmentSlot,
    battle_class: u32,
}

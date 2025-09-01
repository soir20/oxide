use crate::game_server::{ProcessPacketError, ProcessPacketErrorType};

// 8-byte character GUIDs are structured to avoid collisions, from left to right:
// * 1-byte discriminant for character types for 256 unique character types
// * 2-byte NPC index within each type for 65,535 unique NPCs per type, per zone instance
// * 5-byte zone GUID, comprising:
//   * 4-byte zone index for 4,294,967,296 unique instances, per zone template
//   * 1-byte zone template GUID for 256 unique zone templates
//
// Player characters, mounts, and pets are exceptions as they include no zone data in their GUID.
// They always have the special character type discriminant 0x00, 0x01, or 0x2.

pub fn zone_instance_guid(index: u32, template_guid: u8) -> u64 {
    ((index as u64) << 8) | (template_guid as u64)
}

pub fn zone_template_guid(instance_guid: u64) -> u8 {
    (instance_guid & 0xff) as u8
}

pub fn shorten_zone_index(instance_guid: u64) -> u32 {
    ((instance_guid >> 8) & 0xffffffff) as u32
}

pub const AMBIENT_NPC_DISCRIMINANT: u8 = 0x10;
pub const FIXTURE_DISCRIMINANT: u8 = 0x20;
pub const MOUNT_DISCRIMINANT: u8 = 0x30;
pub const SABER_DUEL_DISCRIMINANT: u8 = 0x40;

pub fn npc_guid(discriminant: u8, zone_guid: u64, index: u16) -> u64 {
    ((discriminant as u64) << 56) | ((index as u64) << 40) | zone_guid
}

pub fn player_guid(player_guid: u32) -> u64 {
    player_guid as u64
}

pub fn shorten_player_guid(player_guid: u64) -> Result<u32, ProcessPacketError> {
    if player_guid > u32::MAX as u64 {
        Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!("Player GUID {player_guid} must be <= {}", u32::MAX),
        ))
    } else {
        Ok(player_guid as u32)
    }
}

pub fn mount_guid(rider: u64) -> u64 {
    ((MOUNT_DISCRIMINANT as u64) << 56) | rider
}

pub fn saber_duel_opponent_guid(player_guid: u32) -> u64 {
    ((SABER_DUEL_DISCRIMINANT as u64) << 56) | player_guid as u64
}

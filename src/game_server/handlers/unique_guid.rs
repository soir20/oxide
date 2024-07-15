use crate::game_server::ProcessPacketError;

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

pub fn shorten_zone_template_guid(point_of_interest_id: u32) -> Result<u8, ProcessPacketError> {
    if point_of_interest_id > u8::MAX as u32 {
        Err(ProcessPacketError::CorruptedPacket)
    } else {
        Ok(point_of_interest_id as u8)
    }
}

pub const AMBIENT_NPC_DISCRIMINANT: u8 = 0x10;
pub const FIXTURE_DISCRIMINANT: u8 = 0x20;

pub fn npc_guid(discriminant: u8, zone_guid: u64, index: u16) -> u64 {
    ((discriminant as u64) << 56) | (index as u64) << 40 | zone_guid
}

pub fn player_guid(player_guid: u32) -> u64 {
    player_guid as u64
}

pub fn shorten_player_guid(player_guid: u64) -> Result<u32, ProcessPacketError> {
    if player_guid > u32::MAX as u64 {
        Err(ProcessPacketError::CorruptedPacket)
    } else {
        Ok(player_guid as u32)
    }
}

pub fn mount_guid(rider: u32, mount_id: u32) -> u64 {
    0x0100000000000000u64 | (mount_id as u64) << 32 | (rider as u64)
}

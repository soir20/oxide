use crate::game_server::ProcessPacketError;

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

pub fn fixture_guid(index: u32) -> u64 {
    0x4000000000000000u64 | index as u64
}

pub fn npc_guid(index: u32) -> u64 {
    0x8000000000000000u64 | index as u64
}

pub fn mount_guid(rider: u32, mount_id: u32) -> u64 {
    (mount_id as u64) << 32 | (rider as u64)
}

use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::handlers::unique_guid::player_guid;

use super::{GamePacket, OpCode};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum SquadOpCode {
    MemberStatus = 0xf,
    FullData = 0x12,
    NameplateStatus = 0x17,
}

impl SerializePacket for SquadOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Squad.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u32)]
pub enum SquadEvent {
    Joined = 1,
    Removed = 2,
    Quit = 3,
    Promoted = 4,
    Demoted = 5,
    NoMessage = 6,
    NoMessageDuplicate = 7,
}

impl SerializePacket for SquadEvent {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct SquadMemberStatus {
    pub squad_guid: u64,
    pub player_guid: u64,
    pub new_player_name: String,
    pub new_rank: SquadRank,
    pub online: bool,
    pub member: bool,
    pub event: SquadEvent,
    pub unknown8: String,
}

impl GamePacket for SquadMemberStatus {
    type Header = SquadOpCode;

    const HEADER: Self::Header = SquadOpCode::MemberStatus;
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u32)]
pub enum SquadNameStatus {
    Accepted = 1,
    Rejected = 2,
    Pending = 4,
}

impl SerializePacket for SquadNameStatus {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

pub struct SquadMember {
    pub player_guid: u32,
    pub name: String,
    pub rank: SquadRank,
    pub online: bool,
    pub member: bool,
    pub unknown7: String,
}

impl SerializePacket for SquadMember {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        self.player_guid.serialize(buffer)?;
        player_guid(self.player_guid).serialize(buffer)?;
        self.name.serialize(buffer)?;
        self.rank.serialize(buffer)?;
        self.online.serialize(buffer)?;
        self.member.serialize(buffer)?;
        self.unknown7.serialize(buffer)
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u32)]
pub enum SquadRank {
    Leader = 1,
    General = 2,
    Commander = 3,
    Trooper = 4,
}

impl SerializePacket for SquadRank {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

pub struct SquadRankDefinition {
    pub unknown2: u64,
    pub name_override_id: u32,
    pub rank: SquadRank,
}

impl SerializePacket for SquadRankDefinition {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        // Use the rank as the ID because the client only allows IDs up to 4
        self.rank.serialize(buffer)?;
        self.unknown2.serialize(buffer)?;
        self.rank.serialize(buffer)?;
        self.name_override_id.serialize(buffer)?;
        self.rank.serialize(buffer)
    }
}

pub struct SquadFullData {
    pub squad_guid: u64,
    pub squad_name: String,
    pub unknown4: String,
    pub name_status: SquadNameStatus,
    pub members: Vec<SquadMember>,
    pub rank_definitions: Vec<SquadRankDefinition>,
    pub max_members: u32,
}

impl SerializePacket for SquadFullData {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        self.squad_guid.serialize(buffer)?;
        self.squad_guid.serialize(buffer)?;
        self.squad_name.serialize(buffer)?;
        self.unknown4.serialize(buffer)?;
        self.name_status.serialize(buffer)?;
        self.members.serialize(buffer)?;
        self.rank_definitions.serialize(buffer)?;
        self.max_members.serialize(buffer)
    }
}

impl GamePacket for SquadFullData {
    type Header = SquadOpCode;

    const HEADER: Self::Header = SquadOpCode::FullData;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SquadNameplateStatus {
    pub player_guid: u64,
    pub squad_guid: u64,
    pub show_squad_name: bool,
}

impl GamePacket for SquadNameplateStatus {
    type Header = SquadOpCode;

    const HEADER: Self::Header = SquadOpCode::NameplateStatus;
}

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
    PlayerStatus = 0x17,
}

impl SerializePacket for SquadOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Squad.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SquadMemberStatus {
    pub unknown1: u64,
    pub unknown2: u64,
    pub unknown3: String,
    pub unknown4: u32,
    pub unknown5: bool,
    pub unknown6: bool,
    pub unknown7: u32,
    pub unknown8: String,
}

impl GamePacket for SquadMemberStatus {
    type Header = SquadOpCode;

    const HEADER: Self::Header = SquadOpCode::MemberStatus;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SquadPlayerStatus {
    pub unknown1: u64,
    pub unknown2: u64,
    pub unknown3: bool,
}

impl GamePacket for SquadPlayerStatus {
    type Header = SquadOpCode;

    const HEADER: Self::Header = SquadOpCode::PlayerStatus;
}

pub struct SquadMember {
    pub player_guid: u32,
    pub name: String,
    pub rank_definition_id: u32,
    pub online: bool,
    pub member: bool,
    pub unknown7: String,
}

impl SerializePacket for SquadMember {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        self.player_guid.serialize(buffer)?;
        player_guid(self.player_guid).serialize(buffer)?;
        self.name.serialize(buffer)?;
        self.rank_definition_id.serialize(buffer)?;
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

#[derive(SerializePacket)]
pub struct SquadRankDefinition {
    pub id: u32,
    pub unknown2: u64,
    pub unknown3: u32,
    pub unknown4: u32,
    pub rank: SquadRank,
}

#[derive(SerializePacket)]
pub struct SquadFullData {
    pub unknown1: u64,
    pub unknown2: u64,
    pub squad_name: String,
    pub unknown4: String,
    pub unknown5: u32,
    pub members: Vec<SquadMember>,
    pub rank_definitions: Vec<SquadRankDefinition>,
    pub unknown8: u32,
}

impl GamePacket for SquadFullData {
    type Header = SquadOpCode;

    const HEADER: Self::Header = SquadOpCode::FullData;
}

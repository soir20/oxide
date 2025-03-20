use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{item::BaseAttachmentGroup, GamePacket, OpCode, Pos};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum HousingOpCode {
    SetEditMode = 0x6,
    EnterRequest = 0x10,
    InstanceData = 0x18,
    InstanceList = 0x26,
    FixtureUpdate = 0x27,
    FixtureAsset = 0x29,
    ItemList = 0x2a,
    HouseInfo = 0x2b,
    HouseZoneData = 0x2c,
    InviteNotification = 0x2e,
}

impl SerializePacket for HousingOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Housing.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SetEditMode {
    pub enabled: bool,
}

impl GamePacket for SetEditMode {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::SetEditMode;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct EnterRequest {
    pub house_guid: u64,
    pub unknown1: u32,
    pub unknown2: u32,
}

impl GamePacket for EnterRequest {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::EnterRequest;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PlacedFixture {
    pub fixture_guid: u64,
    pub house_guid: u64,
    pub fixture_asset_id: u32,
    pub unknown2: f32,
    pub pos: Pos,
    pub rot: Pos,
    pub unknown1: Pos,
    pub npc_guid: u64,
    pub item_def_id: u32,
    pub unknown3: u32,
    pub base_attachment_group: BaseAttachmentGroup,
    pub unknown4: String,
    pub unknown5: String,
    pub unknown6: u32,
    pub unknown7: String,
    pub unknown8: bool,
    pub unknown9: u32,
    pub scale: f32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Unknown1 {
    pub fixture_guid: u64,
    pub item_def_id: u32,
    pub unknown1: u32,
    pub unknown2: Vec<u64>,
    pub unknown3: u32,
    pub unknown4: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct FixtureAssetData {
    pub fixture_asset_id: u32,
    pub item_def_id: u32,
    pub unknown2: u32,
    pub model_id: u32,
    pub unknown3: bool,
    pub unknown4: bool,
    pub unknown5: bool,
    pub unknown6: bool,
    pub unknown7: bool,
    pub unknown8: String,
    pub min_scale: f32,
    pub max_scale: f32,
    pub unknown11: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInfo {
    pub edit_mode_enabled: bool,
    pub unknown2: u32,
    pub unknown3: bool,
    pub fixtures: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
}

impl GamePacket for HouseInfo {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::HouseInfo;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseDescription {
    pub owner_guid: u64,
    pub house_guid: u64,
    pub house_name: u32,
    pub player_given_name: String,
    pub owner_name: String,
    pub icon_id: u32,
    pub unknown5: bool,
    pub fixture_count: u32,
    pub unknown7: u64,
    pub furniture_score: u32,
    pub is_locked: bool,
    pub unknown10: String,
    pub unknown11: String,
    pub rating: f32,
    pub total_votes: u32,
    pub is_published: bool,
    pub is_rateable: bool,
    pub unknown16: u32,
    pub unknown17: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseZoneData {
    pub not_editable: bool,
    pub unknown2: u32,
    pub description: HouseDescription,
}

impl GamePacket for HouseZoneData {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::HouseZoneData;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInstanceEntry {
    pub description: HouseDescription,
    pub unknown1: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInstanceList {
    pub instances: Vec<HouseInstanceEntry>,
}

impl GamePacket for HouseInstanceList {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::InstanceList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InstanceUnknown1 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InstanceUnknown2 {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u64,
}

#[derive(Clone, SerializePacket, DeserializePacket)]
pub struct BuildArea {
    pub min: Pos,
    pub max: Pos,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InstancePlacedFixture {
    pub unknown1: u32,
    pub placed_fixture: PlacedFixture,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InnerInstanceData {
    pub house_guid: u64,
    pub owner_guid: u64,
    pub owner_name: String,
    pub unknown3: u64,
    pub house_name: u32,
    pub player_given_name: String,
    pub unknown4: u32,
    pub max_fixtures: u32,
    pub unknown6: u32,
    pub placed_fixture: Vec<InstancePlacedFixture>,
    pub unknown7: bool,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: bool,
    pub unknown11: u32,
    pub unknown12: bool,
    pub build_areas: Vec<BuildArea>,
    pub house_icon: u32,
    pub unknown14: bool,
    pub unknown15: bool,
    pub unknown16: bool,
    pub unknown17: u32,
    pub unknown18: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RoomInstances {
    pub unknown1: Vec<InstanceUnknown1>,
    pub unknown2: Vec<InstanceUnknown2>,
}

pub struct HouseInstanceData {
    pub inner: InnerInstanceData,
    pub rooms: RoomInstances,
}

impl SerializePacket for HouseInstanceData {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner = Vec::new();
        self.inner.serialize(&mut inner)?;
        buffer.write_u32::<LittleEndian>(inner.len() as u32)?;
        buffer.write_all(&inner)?;
        self.rooms.serialize(buffer)?;
        Ok(())
    }
}

impl GamePacket for HouseInstanceData {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::InstanceData;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct FixtureAsset {
    pub model_id: u32,
    pub item_guid: u32,
    pub unknown3: FixtureAssetData,
    pub texture_alias: String,
    pub tint_alias: String,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: String,
    pub unknown9: Vec<u64>,
    pub unknown10: u32,
    pub unknown11: u32,
}

impl GamePacket for FixtureAsset {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::FixtureAsset;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseItemList {
    pub unknown1: Vec<Unknown1>,
    pub unknown2: Vec<FixtureAssetData>,
}

impl GamePacket for HouseItemList {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::ItemList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct FixtureUpdate {
    pub placed_fixture: PlacedFixture,
    pub unknown1: Unknown1,
    pub unknown2: FixtureAssetData,
    pub unknown3: Vec<u64>,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

impl GamePacket for FixtureUpdate {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::FixtureUpdate;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInvite {
    pub unknown1: u64,
    pub owner_name: String,
    pub unknown3: u64,
    pub house_guid: u64,
    pub unknown5: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInviteNotification {
    pub invite: HouseInvite,
    pub unknown1: u64,
}

impl GamePacket for HouseInviteNotification {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::InviteNotification;
}

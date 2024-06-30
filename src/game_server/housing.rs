use std::io::{Cursor, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;
use parking_lot::RwLockReadGuard;

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, Pos};
use crate::game_server::guid::GuidTableHandle;
use crate::game_server::player_update_packet::{
    AddNpc, BaseAttachmentGroup, Icon, WeaponAnimation,
};
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::unique_guid::player_guid;
use crate::game_server::zone::{Fixture, House, Zone};
use crate::game_server::{Broadcast, GameServer, ProcessPacketError};
use crate::teleport_to_zone;

use super::guid::IndexedGuid;
use super::lock_enforcer::{CharacterLockRequest, ZoneLockRequest};
use super::unique_guid::{npc_guid, zone_template_guid, FIXTURE_DISCRIMINANT};

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
    enabled: bool,
}

impl GamePacket for SetEditMode {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::SetEditMode;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct EnterRequest {
    house_guid: u64,
    unknown1: u32,
    unknown2: u32,
}

impl GamePacket for EnterRequest {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::EnterRequest;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PlacedFixture {
    fixture_guid: u64,
    house_guid: u64,
    fixture_asset_id: u32,
    unknown2: f32,
    pos: Pos,
    rot: Pos,
    scale: Pos,
    npc_guid: u64,
    item_def_id: u32,
    unknown3: u32,
    base_attachment_group: BaseAttachmentGroup,
    unknown4: String,
    unknown5: String,
    unknown6: u32,
    unknown7: String,
    unknown8: bool,
    unknown9: u32,
    unknown10: f32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Unknown1 {
    fixture_guid: u64,
    item_def_id: u32,
    unknown1: u32,
    unknown2: Vec<u64>,
    unknown3: u32,
    unknown4: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct FixtureAssetData {
    fixture_asset_id: u32,
    item_def_id: u32,
    unknown2: u32,
    model_id: u32,
    unknown3: bool,
    unknown4: bool,
    unknown5: bool,
    unknown6: bool,
    unknown7: bool,
    unknown8: String,
    min_scale: f32,
    max_scale: f32,
    unknown11: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInfo {
    edit_mode_enabled: bool,
    unknown2: u32,
    unknown3: bool,
    fixtures: u32,
    unknown5: u32,
    unknown6: u32,
    unknown7: u32,
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
    pub icon_id: ImageId,
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
    not_editable: bool,
    unknown2: u32,
    description: HouseDescription,
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
    unknown1: u32,
    unknown2: u32,
    unknown3: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InstanceUnknown2 {
    unknown1: u32,
    unknown2: u32,
    unknown3: u64,
}

#[derive(Clone, SerializePacket, DeserializePacket)]
pub struct BuildArea {
    min: Pos,
    max: Pos,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InstancePlacedFixture {
    unknown1: u32,
    placed_fixture: PlacedFixture,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InnerInstanceData {
    house_guid: u64,
    owner_guid: u64,
    owner_name: String,
    unknown3: u64,
    house_name: u32,
    player_given_name: String,
    unknown4: u32,
    max_fixtures: u32,
    unknown6: u32,
    placed_fixture: Vec<InstancePlacedFixture>,
    unknown7: bool,
    unknown8: u32,
    unknown9: u32,
    unknown10: bool,
    unknown11: u32,
    unknown12: bool,
    build_areas: Vec<BuildArea>,
    house_icon: ImageId,
    unknown14: bool,
    unknown15: bool,
    unknown16: bool,
    unknown17: u32,
    unknown18: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RoomInstances {
    unknown1: Vec<InstanceUnknown1>,
    unknown2: Vec<InstanceUnknown2>,
}

pub struct HouseInstanceData {
    inner: InnerInstanceData,
    rooms: RoomInstances,
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
    model_id: u32,
    item_guid: u32,
    unknown3: FixtureAssetData,
    texture_alias: String,
    tint_alias: String,
    unknown6: u32,
    unknown7: u32,
    unknown8: String,
    unknown9: Vec<u64>,
    unknown10: u32,
    unknown11: u32,
}

impl GamePacket for FixtureAsset {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::FixtureAsset;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseItemList {
    unknown1: Vec<Unknown1>,
    unknown2: Vec<FixtureAssetData>,
}

impl GamePacket for HouseItemList {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::ItemList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct FixtureUpdate {
    placed_fixture: PlacedFixture,
    unknown1: Unknown1,
    unknown2: FixtureAssetData,
    unknown3: Vec<u64>,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
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

fn placed_fixture(index: u16, house_guid: u64, fixture: &Fixture) -> PlacedFixture {
    let fixture_guid = npc_guid(FIXTURE_DISCRIMINANT, house_guid, index);
    PlacedFixture {
        fixture_guid,
        house_guid,
        fixture_asset_id: fixture.item_def_id,
        unknown2: 0.0,
        pos: fixture.pos,
        rot: fixture.rot,
        scale: Pos {
            x: 0.0,
            y: 0.0,
            z: fixture.scale,
            w: 0.0,
        },
        npc_guid: fixture_guid,
        item_def_id: fixture.item_def_id,
        unknown3: 0,
        base_attachment_group: BaseAttachmentGroup {
            unknown1: 0,
            unknown2: "".to_string(),
            unknown3: "".to_string(),
            unknown4: 0,
            unknown5: "".to_string(),
        },
        unknown4: "".to_string(),
        unknown5: "".to_string(),
        unknown6: 0,
        unknown7: "".to_string(),
        unknown8: false,
        unknown9: 0,
        unknown10: 1.0,
    }
}

fn fixture_item_list(
    fixtures: &[Fixture],
    house_guid: u64,
) -> Result<Vec<u8>, SerializePacketError> {
    let mut unknown1 = Vec::new();
    let mut unknown2 = Vec::new();

    for (index, fixture) in fixtures.iter().enumerate() {
        unknown1.push(Unknown1 {
            fixture_guid: npc_guid(FIXTURE_DISCRIMINANT, house_guid, index as u16),
            item_def_id: fixture.item_def_id,
            unknown1: 0,
            unknown2: vec![],
            unknown3: 0,
            unknown4: 0,
        });
        unknown2.push(FixtureAssetData {
            fixture_asset_id: fixture.item_def_id,
            item_def_id: fixture.item_def_id,
            unknown2: 1,
            model_id: fixture.model_id,
            unknown3: false,
            unknown4: false,
            unknown5: false,
            unknown6: false,
            unknown7: false,
            unknown8: "".to_string(),
            min_scale: 0.5,
            max_scale: 2.0,
            unknown11: 0,
        });
    }

    GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: HouseItemList { unknown1, unknown2 },
    })
}

fn fixture_packets(
    house_guid: u64,
    index: u16,
    fixture: &Fixture,
) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    let fixture_guid = npc_guid(FIXTURE_DISCRIMINANT, house_guid, index);
    Ok(vec![
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: FixtureUpdate {
                placed_fixture: placed_fixture(index, house_guid, fixture),
                unknown1: Unknown1 {
                    fixture_guid,
                    item_def_id: fixture.item_def_id,
                    unknown1: 0,
                    unknown2: vec![],
                    unknown3: 0,
                    unknown4: 0,
                },
                unknown2: FixtureAssetData {
                    fixture_asset_id: fixture.item_def_id,
                    item_def_id: fixture.item_def_id,
                    unknown2: 1,
                    model_id: fixture.model_id,
                    unknown3: false,
                    unknown4: false,
                    unknown5: false,
                    unknown6: true,
                    unknown7: false,
                    unknown8: "".to_string(),
                    min_scale: 0.5,
                    max_scale: 2.0,
                    unknown11: 0,
                },
                unknown3: vec![],
                unknown4: 0,
                unknown5: 0,
                unknown6: 0,
            },
        })?,
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AddNpc {
                guid: fixture_guid,
                name_id: 0,
                model_id: fixture.model_id,
                unknown3: false,
                unknown4: 408679,
                unknown5: 13951728,
                unknown6: 1,
                scale: fixture.scale,
                pos: fixture.pos,
                rot: fixture.rot,
                unknown8: 1,
                attachments: vec![],
                is_not_targetable: 1,
                unknown10: 0,
                texture_name: fixture.texture_name.clone(),
                tint_name: "".to_string(),
                tint_id: 0,
                unknown11: true,
                offset_y: 0.0,
                composite_effect: 0,
                weapon_animation: WeaponAnimation::None,
                name_override: "".to_string(),
                hide_name: true,
                name_offset_x: 0.0,
                name_offset_y: 0.0,
                name_offset_z: 0.0,
                terrain_object_id: 0,
                invisible: false,
                unknown20: 0.0,
                unknown21: false,
                interactable_size_pct: 100,
                unknown23: -1,
                unknown24: -1,
                active_animation_slot: -1,
                unknown26: true,
                ignore_position: true,
                sub_title_id: 0,
                active_animation_slot2: 0,
                head_model_id: 0,
                effects: vec![],
                disable_interact_popup: false,
                unknown33: 0,
                unknown34: false,
                show_health: false,
                hide_despawn_fade: false,
                ignore_rotation_and_shadow: true,
                base_attachment_group: BaseAttachmentGroup {
                    unknown1: 0,
                    unknown2: "".to_string(),
                    unknown3: "".to_string(),
                    unknown4: 0,
                    unknown5: "".to_string(),
                },
                unknown39: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                unknown40: 0,
                unknown41: -1,
                unknown42: 0,
                collision: true,
                unknown44: 0,
                npc_type: 0,
                unknown46: 0.0,
                target: 0,
                unknown50: vec![],
                rail_id: 0,
                rail_speed: 0.0,
                rail_origin: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                unknown54: 0,
                rail_unknown1: 0.0,
                rail_unknown2: 0.0,
                rail_unknown3: 0.0,
                attachment_group_unknown: "".to_string(),
                unknown59: "".to_string(),
                unknown60: "".to_string(),
                override_terrain_model: false,
                hover_glow: 0,
                hover_description: 0,
                fly_over_effect: 0,
                unknown65: 0,
                unknown66: 0,
                unknown67: 0,
                disable_move_to_interact: false,
                unknown69: 0.0,
                unknown70: 0.0,
                unknown71: 0,
                icon_id: Icon::None,
            },
        })?,
    ])
}

pub fn prepare_init_house_packets(
    sender: u32,
    zone: &RwLockReadGuard<Zone>,
    house: &House,
) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    if house.is_locked && sender != house.owner {
        return Err(ProcessPacketError::CorruptedPacket);
    }

    let mut packets = vec![
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: HouseZoneData {
                not_editable: sender != house.owner,
                unknown2: 0,
                description: HouseDescription {
                    owner_guid: player_guid(house.owner),
                    house_guid: zone.guid(),
                    house_name: zone.template_name,
                    player_given_name: house.custom_name.clone(),
                    owner_name: house.owner_name.clone(),
                    icon_id: zone.icon,
                    unknown5: false,
                    fixture_count: house.fixtures.len() as u32,
                    unknown7: 0,
                    furniture_score: 0,
                    is_locked: house.is_locked,
                    unknown10: "".to_string(),
                    unknown11: "".to_string(),
                    rating: house.rating,
                    total_votes: house.total_votes,
                    is_published: house.is_published,
                    is_rateable: house.is_rateable,
                    unknown16: 0,
                    unknown17: 0,
                },
            },
        })?,
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: HouseInstanceData {
                inner: InnerInstanceData {
                    house_guid: zone.guid(),
                    owner_guid: player_guid(house.owner),
                    owner_name: house.owner_name.clone(),
                    unknown3: 0,
                    house_name: zone.template_name,
                    player_given_name: house.custom_name.clone(),
                    unknown4: 0,
                    max_fixtures: 10000,
                    unknown6: 0,
                    placed_fixture: vec![],
                    unknown7: false,
                    unknown8: 0,
                    unknown9: 0,
                    unknown10: false,
                    unknown11: 0,
                    unknown12: false,
                    build_areas: house.build_areas.clone(),
                    house_icon: zone.icon,
                    unknown14: false,
                    unknown15: false,
                    unknown16: false,
                    unknown17: 0,
                    unknown18: 0,
                },
                rooms: RoomInstances {
                    unknown1: vec![],
                    unknown2: vec![],
                },
            },
        })?,
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: HouseInfo {
                edit_mode_enabled: false,
                unknown2: 0,
                unknown3: true,
                fixtures: house.fixtures.len() as u32,
                unknown5: 0,
                unknown6: 0,
                unknown7: 0,
            },
        })?,
    ];

    for (index, fixture) in house.fixtures.iter().enumerate() {
        packets.append(&mut fixture_packets(zone.guid(), index as u16, fixture)?);
    }

    Ok(packets)
}

pub fn process_housing_packet(
    sender: u32,
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match HousingOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            HousingOpCode::SetEditMode => {
                let set_edit_mode: SetEditMode = DeserializePacket::deserialize(cursor)?;
                game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
                    read_guids: Vec::new(),
                    write_guids: Vec::new(),
                    character_consumer: |characters_table_read_handle, _, _, zones_lock_enforcer| {
                        let packets = if let Some((instance_guid, _)) = characters_table_read_handle.index(player_guid(sender)) {
                            zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                                read_guids: vec![instance_guid],
                                write_guids: Vec::new(),
                                zone_consumer: |_, zones_read, _| {
                                    if let Some(zone_read_handle) = zones_read.get(&instance_guid){
                                        if let Some(house) = &zone_read_handle.house_data {
                                            if house.owner == sender {
                                                Ok(vec![GamePacket::serialize(&TunneledPacket {
                                                    unknown1: true,
                                                    inner: HouseInfo {
                                                        edit_mode_enabled: set_edit_mode.enabled,
                                                        unknown2: 0,
                                                        unknown3: true,
                                                        fixtures: house.fixtures.len() as u32,
                                                        unknown5: 0,
                                                        unknown6: 0,
                                                        unknown7: 0,
                                                    },
                                                })?])
                                            } else {
                                                println!(
                                                    "Player {} tried to set edit mode in a house they don't own",
                                                    sender
                                                );
                                                Err(ProcessPacketError::CorruptedPacket)
                                            }
                                        } else {
                                            println!(
                                                "Player {} tried to set edit mode outside of a house",
                                                sender
                                            );
                                            Err(ProcessPacketError::CorruptedPacket)
                                        }
                                    } else {
                                        println!(
                                            "Player {} tried to set edit mode but is not in any zone",
                                            sender
                                        );
                                        Err(ProcessPacketError::CorruptedPacket)
                                    }
                                },
                            })
                        } else {
                            println!("Non-existent player {} tried to set edit mode", sender);
                            Err(ProcessPacketError::CorruptedPacket)
                        }?;

                        Ok(vec![Broadcast::Single(sender, packets)])
                    },
                })
            }
            HousingOpCode::EnterRequest => {
                let enter_request: EnterRequest = DeserializePacket::deserialize(cursor)?;

                game_server.lock_enforcer().write_characters(
                    |characters_table_write_handle, zones_lock_enforcer| {
                        zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                            // Create the house instance if it does not already exist
                            if zones_table_write_handle
                                .get(enter_request.house_guid)
                                .is_none()
                            {
                                let template_guid = zone_template_guid(enter_request.house_guid);
                                if let Some(template) =
                                    game_server.read_zone_templates().get(&template_guid)
                                {
                                    zones_table_write_handle.insert(Zone::new_house(
                                        enter_request.house_guid,
                                        template,
                                        lookup_house(sender, enter_request.house_guid)?,
                                        characters_table_write_handle,
                                    ));
                                } else {
                                    println!(
                                        "Tried to enter house with unknown template {}",
                                        template_guid
                                    );
                                    return Err(ProcessPacketError::CorruptedPacket);
                                }
                            }

                            if let Some(zone_read_handle) =
                                zones_table_write_handle.get(enter_request.house_guid)
                            {
                                teleport_to_zone!(
                                    characters_table_write_handle,
                                    sender,
                                    &zone_read_handle.read(),
                                    None,
                                    None,
                                    game_server.mounts()
                                )
                            } else {
                                println!("Unable to create house {}", enter_request.house_guid);
                                Err(ProcessPacketError::CorruptedPacket)
                            }
                        })
                    },
                )
            }
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                println!("Unimplemented housing packet: {:?}, {:x?}", op_code, buffer);
                Ok(Vec::new())
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            println!("Unknown housing packet: {}, {:x?}", raw_op_code, buffer);
            Ok(Vec::new())
        }
    }
}

pub fn lookup_house(sender: u32, house_guid: u64) -> Result<House, ProcessPacketError> {
    println!("Found test house {}", house_guid);
    Ok(House {
        owner: sender,
        owner_name: "BLASTER NICESHOt".to_string(),
        custom_name: "Blaster's Test Lot".to_string(),
        rating: 3.5,
        total_votes: 100,
        fixtures: vec![
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 0.0,
                    z: 481.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 6,
                model_id: 1417,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 0.0,
                    z: 483.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 7,
                model_id: 1419,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 0.0,
                    z: 485.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 7,
                model_id: 1419,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 0.0,
                    z: 487.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 8,
                model_id: 1420,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 0.0,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 9,
                model_id: 1418,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 0.5,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 9,
                model_id: 1418,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 1.0,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 9,
                model_id: 1418,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 495.0,
                    y: 1.5,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 9,
                model_id: 1418,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 490.0,
                    y: 0.0,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 10,
                model_id: 1416,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 490.0,
                    y: 0.5,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 10,
                model_id: 1416,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 490.0,
                    y: 1.0,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 10,
                model_id: 1416,
                texture_name: "".to_string(),
            },
            Fixture {
                pos: Pos {
                    x: 490.0,
                    y: 1.5,
                    z: 475.5,
                    w: 1.0,
                },
                rot: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                scale: 1.0,
                item_def_id: 10,
                model_id: 1416,
                texture_name: "".to_string(),
            },
        ],
        build_areas: vec![BuildArea {
            min: Pos {
                x: 384.0,
                y: -1.0,
                z: 448.0,
                w: 0.0,
            },
            max: Pos {
                x: 512.0,
                y: 100.0,
                z: 512.0,
                w: 0.0,
            },
        }],
        is_locked: false,
        is_published: false,
        is_rateable: false,
    })
}

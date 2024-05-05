use std::io::{Cursor, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::TryFromPrimitive;
use parking_lot::RwLockReadGuard;

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::{GameServer, ProcessPacketError};
use crate::game_server::character_guid::{fixture_guid, player_guid};
use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, Pos};
use crate::game_server::guid::Guid;
use crate::game_server::player_update_packet::{AddNpc, BaseAttachmentGroup, Icon, make_test_npc, WeaponAnimation};
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::ui::ExecuteScriptWithParams;
use crate::game_server::zone::{Fixture, House, teleport_to_zone, Zone};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum HousingOpCode {
    EnterRequest             = 0x10,
    InstanceData             = 0x18,
    InstanceList             = 0x26,
    FixtureUpdate            = 0x27,
    FixtureAsset             = 0x29,
    ItemList                 = 0x2a,
    HouseInfo                = 0x2b,
    HouseZoneData            = 0x2c,
    InviteNotification       = 0x2e
}

impl SerializePacket for HousingOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Housing.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct EnterRequest {
    house_guid: u64,
    unknown1: u32,
    unknown2: u32
}

impl GamePacket for EnterRequest {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::EnterRequest;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PlacedFixture {
    fixture_guid: u64,
    house_guid: u64,
    unknown_id: u32,
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
    unknown4: u32
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Unknown2 {
    unknown_id: u32,
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
    pub unknown17: u32
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseZoneData {
    not_editable: bool,
    unknown2: u32,
    description: HouseDescription
}

impl GamePacket for HouseZoneData {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::HouseZoneData;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInstanceEntry {
    pub description: HouseDescription,
    pub unknown1: u64
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInstanceList {
    pub instances: Vec<HouseInstanceEntry>
}

impl GamePacket for HouseInstanceList {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::InstanceList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InstanceUnknown1 {
    unknown1: u32,
    unknown2: u32,
    unknown3: u64
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InstanceUnknown2 {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u64
}

#[derive(Clone, SerializePacket, DeserializePacket)]
pub struct BuildArea {
    min: Pos,
    max: Pos
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
    unknown5: u32,
    unknown6: u32,
    placed_fixture: Vec<PlacedFixture>,
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
    unknown1: Vec<Unknown1>,
    unknown2: Vec<Unknown2>,
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
    unknown3: Unknown2,
    texture_alias: String,
    tint_alias: String,
    unknown6: u32,
    unknown7: u32,
    unknown8: String,
    unknown9: Vec<u64>,
    unknown10: u32,
    unknown11: u32
}

impl GamePacket for FixtureAsset {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::FixtureAsset;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseItemList {
    unknown1: Vec<Unknown1>,
    unknown2: Vec<Unknown2>
}

impl GamePacket for HouseItemList {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::ItemList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct FixtureUpdate {
    placed_fixture: PlacedFixture,
    unknown1: Unknown1,
    unknown2: Unknown2,
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
    pub unknown5: u64
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseInviteNotification {
    pub invite: HouseInvite,
    pub unknown1: u64
}

impl GamePacket for HouseInviteNotification {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::InviteNotification;
}

fn placed_fixture(index: u32, house_guid: u64, fixture: &Fixture) -> PlacedFixture {
    let fixture_guid = fixture_guid(index);
    PlacedFixture {
        fixture_guid,
        house_guid,
        unknown_id: 0,
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

fn fixture_item_list(fixtures: &Vec<Fixture>) -> Result<Vec<u8>, SerializePacketError> {
    let mut unknown1 = Vec::new();
    let mut unknown2 = Vec::new();

    for (index, fixture) in fixtures.iter().enumerate() {
        unknown1.push(Unknown1 {
            fixture_guid: fixture_guid(index as u32),
            item_def_id: fixture.item_def_id,
            unknown1: 0,
            unknown2: vec![],
            unknown3: 0,
            unknown4: 0,
        });
        unknown2.push(Unknown2 {
            unknown_id: 0,
            item_def_id: fixture.item_def_id,
            unknown2: 0,
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

    GamePacket::serialize(
        &TunneledPacket {
            unknown1: true,
            inner: HouseItemList {
                unknown1,
                unknown2
            },
        }
    )
}

fn fixture_npc(index: u32, fixture: &Fixture) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    Ok(
        vec![
            GamePacket::serialize(
                &TunneledPacket {
                    unknown1: true,
                    inner: AddNpc {
                        guid: fixture_guid(index),
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
                        unknown31: vec![],
                        disable_interact_popup: false,
                        unknown33: 0,
                        unknown34: false,
                        show_health: false,
                        unknown36: false,
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
                    }
                }
            )?
        ]
    )
}

pub fn prepare_init_house_packets(sender: u32, zone: &RwLockReadGuard<Zone>,
                                  house: &House) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    if house.is_locked && sender != house.owner {
        return Err(ProcessPacketError::CorruptedPacket);
    }

    let mut packets = vec![
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseZoneData {
                    not_editable: sender != house.owner,
                    unknown2: 0,
                    description: HouseDescription {
                        owner_guid: player_guid(house.owner),
                        house_guid: zone.guid(),
                        house_name: zone.template_name,
                        player_given_name: house.name.clone(),
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
                    }
                },
            }
        )?,
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseInstanceData {
                    inner: InnerInstanceData {
                        house_guid: zone.guid(),
                        owner_guid: player_guid(house.owner),
                        owner_name: house.owner_name.clone(),
                        unknown3: 0,
                        house_name: zone.template_name,
                        player_given_name: house.name.clone(),
                        unknown4: 0,
                        unknown5: 0,
                        unknown6: 0,
                        placed_fixture: house.fixtures.iter().enumerate()
                            .map(|(index, fixture)| placed_fixture(index as u32, zone.guid(), fixture))
                            .collect(),
                        unknown7: false,
                        unknown8: 0,
                        unknown9: 0,
                        unknown10: false,
                        unknown11: 0,
                        unknown12: false,
                        build_areas: house.build_areas.clone(),
                        house_icon: 0,
                        unknown14: false,
                        unknown15: false,
                        unknown16: false,
                        unknown17: 0,
                        unknown18: 0,
                    },
                    rooms: RoomInstances {
                        unknown1: vec![],
                        unknown2: vec![],
                    }
                }
            }
        )?,
        fixture_item_list(&house.fixtures)?,
        GamePacket::serialize(
            &TunneledPacket {
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
            }
        )?
    ];

    for (index, fixture) in house.fixtures.iter().enumerate() {
        packets.append(&mut fixture_npc(index as u32, fixture)?);
    }

    Ok(packets)
}

pub fn process_housing_packet(sender: u32, game_server: &GameServer, cursor: &mut Cursor<&[u8]>) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    let raw_op_code = cursor.read_u16::<LittleEndian>()?;
    match HousingOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            HousingOpCode::EnterRequest => {
                let enter_request: EnterRequest = DeserializePacket::deserialize(cursor)?;

                let zones = game_server.read_zones();
                if let Some(zone_guid) = GameServer::zone_with_character(&zones, player_guid(sender)) {
                    if let Some(zone) = zones.get(zone_guid) {
                        let zone_read_handle = zone.read();
                        Ok(teleport_to_zone(
                            &zones,
                            zone_read_handle,
                            sender,
                            enter_request.house_guid,
                            None,
                            None
                        )?)
                    } else {
                        println!("Received enter request for unknown house {}", enter_request.house_guid);
                        Err(ProcessPacketError::CorruptedPacket)
                    }
                } else {
                    println!("Received teleport request for player not in any zone");
                    Err(ProcessPacketError::CorruptedPacket)
                }
            },
            _ => {
                println!("Unimplemented housing packet: {:?}", op_code);
                Ok(Vec::new())
            }
        },
        Err(_) => {
            println!("Unknown housing packet: {}", raw_op_code);
            Ok(Vec::new())
        }
    }
}

pub fn make_test_fixture_packets() -> Result<Vec<Vec<u8>>, SerializePacketError> {
    Ok(vec![
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseZoneData {
                    not_editable: false,
                    unknown2: 0,
                    description: HouseDescription {
                        owner_guid: 1,
                        house_guid: 101,
                        house_name: 0,
                        player_given_name: "Blaster's Amazing Lot".to_string(),
                        owner_name: "Blaster".to_string(),
                        icon_id: 0,
                        unknown5: false,
                        fixture_count: 0,
                        unknown7: 0,
                        furniture_score: 0,
                        is_locked: false,
                        unknown10: "".to_string(),
                        unknown11: "".to_string(),
                        rating: 0.0,
                        total_votes: 0,
                        is_published: false,
                        is_rateable: true,
                        unknown16: 0,
                        unknown17: 0,
                    }
                },
            }
        )?,
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseInstanceData {
                    inner: InnerInstanceData {
                        house_guid: 101,
                        owner_guid: 1,
                        owner_name: "Blaster".to_string(),
                        unknown3: 0,
                        house_name: 0,
                        player_given_name: "Blaster's Amazing Lot".to_string(),
                        unknown4: 0,
                        unknown5: 0,
                        unknown6: 0,
                        placed_fixture: vec![],
                        unknown7: false,
                        unknown8: 0,
                        unknown9: 0,
                        unknown10: false,
                        unknown11: 0,
                        unknown12: false,
                        build_areas: vec![
                            BuildArea {
                                min: Pos {
                                    x: 787.3,
                                    y: 71.93376,
                                    z: 1446.956,
                                    w: 0.0,
                                },
                                max: Pos {
                                    x: 987.3,
                                    y: 271.93376,
                                    z: 1646.956,
                                    w: 0.0,
                                },
                            }
                        ],
                        house_icon: 0,
                        unknown14: false,
                        unknown15: false,
                        unknown16: false,
                        unknown17: 0,
                        unknown18: 0,
                    },
                    rooms: RoomInstances {
                        unknown1: vec![],
                        unknown2: vec![],
                    }
                }
            }
        )?,
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: ExecuteScriptWithParams {
                    script_name: "HouseHandler.setEditMode".to_string(),
                    params: vec!["1".to_string()],
                }
            }
        )?,
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseInfo {
                    edit_mode_enabled: true,
                    unknown2: 0,
                    unknown3: true,
                    fixtures: 0,
                    unknown5: 0,
                    unknown6: 0,
                    unknown7: 0,
                },
            }
        )?,
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseItemList {
                    unknown1: vec![
                        Unknown1 {
                            fixture_guid: 100,
                            item_def_id: 6,
                            unknown1: 0,
                            unknown2: vec![],
                            unknown3: 0,
                            unknown4: 0,
                        }
                    ],
                    unknown2: vec![
                        Unknown2 {
                            unknown_id: 0x22,
                            item_def_id: 6,
                            unknown2: 1,
                            model_id: 458,
                            unknown3: false,
                            unknown4: false,
                            unknown5: false,
                            unknown6: true,
                            unknown7: false,
                            unknown8: "".to_string(),
                            min_scale: 0.5,
                            max_scale: 2.0,
                            unknown11: 0,
                        }
                    ],
                },
            }
        )?,
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: FixtureAsset {
                model_id: 458,
                item_guid: 6,
                unknown3: Unknown2 {
                    unknown_id: 0x22,
                    item_def_id: 6,
                    unknown2: 1,
                    model_id: 458,
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
                texture_alias: "".to_string(),
                tint_alias: "".to_string(),
                unknown6: 0,
                unknown7: 0,
                unknown8: "".to_string(),
                unknown9: vec![],
                unknown10: 0,
                unknown11: 0,
            },
        })?,
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: FixtureUpdate {
                placed_fixture: PlacedFixture {
                    fixture_guid: 100,
                    house_guid: 101,
                    unknown_id: 0x22,
                    unknown2: 0.0,
                    pos: Pos {
                        x: 887.3,
                        y: 171.93376,
                        z: 1546.956,
                        w: 1.0,
                    },
                    rot: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                    scale: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                        w: 0.0,
                    },
                    npc_guid: 102,
                    item_def_id: 6,
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
                },
                unknown1: Unknown1 {
                    fixture_guid: 100,
                    item_def_id: 6,
                    unknown1: 0,
                    unknown2: vec![],
                    unknown3: 0,
                    unknown4: 0,
                },
                unknown2: Unknown2 {
                    unknown_id: 0x22,
                    item_def_id: 6,
                    unknown2: 1,
                    model_id: 458,
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
            }
        })?,
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseInfo {
                    edit_mode_enabled: true,
                    unknown2: 0,
                    unknown3: true,
                    fixtures: 1,
                    unknown5: 0,
                    unknown6: 0,
                    unknown7: 0,
                },
            }
        )?,
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: make_test_npc()
        })?,
    ])
}
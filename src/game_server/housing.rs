use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, Pos};
use crate::game_server::player_update_packet::{BaseAttachmentGroup, make_test_npc};
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::ui::ExecuteScriptWithParams;

#[derive(Copy, Clone, Debug)]
pub enum HousingOpCode {
    InstanceData             = 0x18,
    FixtureUpdate            = 0x27,
    FixtureAsset             = 0x29,
    ItemList                 = 0x2a,
    HouseInfo                = 0x2b,
    HouseZoneData            = 0x2c,
}

impl SerializePacket for HousingOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Housing.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
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
pub struct HouseZoneData {
    not_editable: bool,
    unknown2: u32,
    owner_guid: u64,
    house_guid: u64,
    house_name: u32,
    player_given_name: String,
    owner_name: String,
    icon_id: ImageId,
    unknown5: bool,
    unknown6: u32,
    unknown7: u64,
    unknown8: u32,
    unknown9: bool,
    unknown10: String,
    unknown11: String,
    unknown12: u32,
    unknown13: u32,
    is_published: bool,
    is_rateable: bool,
    unknown16: u32,
    unknown17: u32
}

impl GamePacket for HouseZoneData {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::HouseZoneData;
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

#[derive(SerializePacket, DeserializePacket)]
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

pub fn make_test_fixture_packets() -> Result<Vec<Vec<u8>>, SerializePacketError> {
    Ok(vec![
        GamePacket::serialize(
            &TunneledPacket {
                unknown1: true,
                inner: HouseZoneData {
                    not_editable: false,
                    unknown2: 0,
                    owner_guid: 1,
                    house_guid: 101,
                    house_name: 0,
                    player_given_name: "Blaster's Amazing Lot".to_string(),
                    owner_name: "Blaster".to_string(),
                    icon_id: 0,
                    unknown5: false,
                    unknown6: 0,
                    unknown7: 0,
                    unknown8: 0,
                    unknown9: false,
                    unknown10: "".to_string(),
                    unknown11: "".to_string(),
                    unknown12: 0,
                    unknown13: 0,
                    is_published: false,
                    is_rateable: true,
                    unknown16: 0,
                    unknown17: 0,
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

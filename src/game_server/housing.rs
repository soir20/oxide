use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use crate::game_server::game_packet::{GamePacket, ImageId, OpCode, Pos};
use crate::game_server::player_update_packet::BaseAttachmentGroup;
use crate::game_server::tunnel::TunneledPacket;

#[derive(Copy, Clone, Debug)]
pub enum HousingOpCode {
    FixtureUpdate            = 0x27,
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
    unknown9: u32,
    unknown10: u32,
    unknown11: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HouseZoneData {
    is_published: bool,
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
    unknown14: bool,
    is_rateable: bool,
    unknown16: u32,
    unknown17: u32
}

impl GamePacket for HouseZoneData {
    type Header = HousingOpCode;
    const HEADER: Self::Header = HousingOpCode::HouseZoneData;
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
                    is_published: false,
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
                    unknown14: false,
                    is_rateable: true,
                    unknown16: 0,
                    unknown17: 0,
                },
            }
        )?,
        GamePacket::serialize(
            &TunneledPacket {
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
                        unknown10: 0.0,
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
                        unknown9: 0,
                        unknown10: 0,
                        unknown11: 0,
                    },
                    unknown3: vec![],
                    unknown4: 0,
                    unknown5: 0,
                    unknown6: 0,
                },
            }
        )?
    ])
}

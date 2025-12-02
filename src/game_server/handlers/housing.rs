use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::{
    game_server::{
        packets::{
            housing::{
                BuildArea, EnterRequest, FixtureAssetData, FixtureUpdate, HouseDescription,
                HouseInfo, HouseInstanceData, HouseInstanceEntry, HouseInstanceList, HouseItemList,
                HouseZoneData, HousingOpCode, InnerInstanceData, PlacedFixture,
                RequestPlayerHouseList, RoomInstances, SetEditMode, Unknown1,
            },
            item::{BaseAttachmentGroup, WieldType},
            player_update::{AddNpc, Hostility, Icon, PhysicsState},
            tunnel::TunneledPacket,
            GamePacket, Pos, Target,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    info, teleport_to_zone,
};

use super::{
    character::{Character, CurrentFixture, PreviousFixture},
    guid::{GuidTableIndexer, IndexedGuid},
    lock_enforcer::{CharacterLockRequest, ZoneLockEnforcer, ZoneLockRequest},
    unique_guid::{
        npc_guid, player_guid, zone_instance_guid, zone_template_guid, FIXTURE_DISCRIMINANT,
    },
    zone::{House, ZoneInstance},
};

fn placed_fixture(
    fixture_guid: u64,
    house_guid: u64,
    fixture: &CurrentFixture,
    pos: Pos,
    rot: Pos,
    scale: f32,
) -> PlacedFixture {
    PlacedFixture {
        fixture_guid,
        house_guid,
        fixture_asset_id: fixture.item_def_id,
        unknown2: 0.0,
        pos,
        rot,
        unknown1: Pos {
            x: 0.0,
            y: 0.0,
            z: 0.0,
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
        scale,
    }
}

#[allow(dead_code)]
fn fixture_item_list(fixtures: &[PreviousFixture], house_guid: u64) -> Vec<u8> {
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

pub fn fixture_packets(
    house_guid: u64,
    fixture_guid: u64,
    fixture: &CurrentFixture,
    pos: Pos,
    rot: Pos,
    scale: f32,
) -> Vec<Vec<u8>> {
    vec![
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: FixtureUpdate {
                placed_fixture: placed_fixture(fixture_guid, house_guid, fixture, pos, rot, scale),
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
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AddNpc {
                guid: fixture_guid,
                name_id: 0,
                model_id: fixture.model_id,
                unknown3: false,
                chat_text_color: Character::DEFAULT_CHAT_TEXT_COLOR,
                chat_bubble_color: Character::DEFAULT_CHAT_BUBBLE_COLOR,
                chat_scale: 1,
                scale,
                pos,
                rot,
                spawn_animation_id: 1,
                attachments: vec![],
                hostility: Hostility::Neutral,
                unknown10: 0,
                texture_alias: fixture.texture_alias.clone(),
                tint_name: "".to_string(),
                tint_id: 0,
                unknown11: true,
                offset_y: 0.0,
                composite_effect_id: 0,
                wield_type: WieldType::None,
                name_override: "".to_string(),
                hide_name: true,
                name_offset_x: 0.0,
                name_offset_y: 0.0,
                name_offset_z: 0.0,
                terrain_object_id: 0,
                enable_attachments: false,
                speed: 0.0,
                unknown21: false,
                interactable_size_pct: 100,
                walk_animation_id: -1,
                sprint_animation_id: -1,
                stand_animation_id: -1,
                unknown26: true,
                disable_gravity: true,
                sub_title_id: 0,
                one_shot_animation_id: -1,
                temporary_model: 0,
                effects: vec![],
                disable_interact_popup: false,
                unused_death_animation_id: 0,
                unknown34: false,
                show_health: false,
                hide_despawn_fade: false,
                enable_tilt: true,
                base_attachment_group: BaseAttachmentGroup {
                    unknown1: 0,
                    unknown2: "".to_string(),
                    unknown3: "".to_string(),
                    unknown4: 0,
                    unknown5: "".to_string(),
                },
                tilt: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                unknown40: 0,
                bounce_area_id: -1,
                image_set_id: 0,
                clickable: true,
                rider_guid: 0,
                physics: PhysicsState::default(),
                interact_popup_radius: 0.0,
                target: Target::default(),
                variables: vec![],
                rail_id: 0,
                rail_elapsed_seconds: 0.0,
                rail_offset: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                unknown54: 0,
                rail_unknown1: 0.0,
                rail_unknown2: 0.0,
                auto_interact_radius: 0.0,
                head_customization_override: "".to_string(),
                hair_customization_override: "".to_string(),
                body_customization_override: "".to_string(),
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
        }),
    ]
}

pub fn prepare_init_house_packets(
    sender: u32,
    zone: &ZoneInstance,
    house: &House,
) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    if house.is_locked && sender != house.owner {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {} tried to enter locked house {}",
                sender,
                zone.guid()
            ),
        ));
    }

    Ok(vec![
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
        }),
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
        }),
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
        }),
    ])
}

pub fn process_housing_packet(
    sender: u32,
    game_server: &GameServer,
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u16 = DeserializePacket::deserialize(cursor)?;
    match HousingOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            HousingOpCode::SetEditMode => {
                let set_edit_mode: SetEditMode = DeserializePacket::deserialize(cursor)?;
                game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
                    read_guids: Vec::new(),
                    write_guids: Vec::new(),
                    character_consumer: |characters_table_read_handle, _, _, minigame_data_lock_enforcer| {
                        let Some((_, instance_guid, _)) = characters_table_read_handle.index1(player_guid(sender)) else {
                            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Non-existent player {sender} tried to set edit mode")));
                        };

                        let zones_lock_enforcer: ZoneLockEnforcer = minigame_data_lock_enforcer.into();
                        let packets = zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                            read_guids: vec![instance_guid],
                            write_guids: Vec::new(),
                            zone_consumer: |_, zones_read, _| {
                                let Some(zone_read_handle) = zones_read.get(&instance_guid) else {
                                    return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!(
                                        "Player {sender} tried to set edit mode but is not in any zone"
                                    )));
                                };

                                let Some(house) = &zone_read_handle.house_data else {
                                    return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!(
                                        "Player {sender} tried to set edit mode outside of a house"
                                    )));
                                };

                                if house.owner != sender {
                                    return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!(
                                        "Player {sender} tried to set edit mode in a house they don't own"
                                    )));
                                }

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
                                })])
                            },
                        })?;

                        Ok(vec![Broadcast::Single(sender, packets)])
                    },
                })
            }
            HousingOpCode::RequestPlayerHouseList => {
                let request = RequestPlayerHouseList::deserialize(cursor)?;
                Ok(vec![Broadcast::Single(
                    sender,
                    vec![GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: HouseInstanceList {
                            instances: vec![HouseInstanceEntry {
                                description: HouseDescription {
                                    owner_guid: request.player_guid,
                                    house_guid: zone_instance_guid(0, 100),
                                    house_name: 1987,
                                    player_given_name: "Blaster's Mustafar Lot".to_string(),
                                    owner_name: "BLASTER NICESHOT".to_string(),
                                    icon_id: 4209,
                                    unknown5: true,
                                    fixture_count: 1,
                                    unknown7: 0,
                                    furniture_score: 3,
                                    is_locked: false,
                                    unknown10: "".to_string(),
                                    unknown11: "".to_string(),
                                    rating: 4.5,
                                    total_votes: 5,
                                    is_published: false,
                                    is_rateable: false,
                                    unknown16: 0,
                                    unknown17: 0,
                                },
                                unknown1: request.player_guid,
                            }],
                        },
                    })],
                )])
            }
            HousingOpCode::EnterRequest => {
                let enter_request: EnterRequest = DeserializePacket::deserialize(cursor)?;

                game_server.lock_enforcer().write_characters(
                    |characters_table_write_handle, minigame_data_lock_enforcer| {
                        let zones_lock_enforcer: ZoneLockEnforcer<'_> =
                            minigame_data_lock_enforcer.into();
                        zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                            // Create the house instance if it does not already exist
                            if zones_table_write_handle
                                .get(enter_request.house_guid)
                                .is_none()
                            {
                                let template_guid = zone_template_guid(enter_request.house_guid);
                                let Some(template) =
                                    game_server.read_zone_templates().get(&template_guid)
                                else {
                                    return Err(ProcessPacketError::new(
                                        ProcessPacketErrorType::ConstraintViolated,
                                        format!(
                                            "Tried to enter house with unknown template {template_guid}"
                                        ),
                                    ));
                                };

                                zones_table_write_handle.insert(ZoneInstance::new_house(
                                    enter_request.house_guid,
                                    template,
                                    lookup_house(sender, enter_request.house_guid)?,
                                    characters_table_write_handle,
                                ));
                            }

                            let Some(zone_read_handle) =
                                zones_table_write_handle.get(enter_request.house_guid)
                            else {
                                return Err(ProcessPacketError::new(
                                    ProcessPacketErrorType::ConstraintViolated,
                                    format!("Unable to create house {}", enter_request.house_guid),
                                ));
                            };

                            teleport_to_zone!(
                                characters_table_write_handle,
                                sender,
                                zones_table_write_handle,
                                &zone_read_handle.read(),
                                None,
                                None,
                                game_server.mounts(),
                            )
                        })
                    },
                )
            }
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unimplemented housing packet: {op_code:?}, {buffer:x?}"),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unknown housing packet: {raw_op_code}, {buffer:x?}"),
            ))
        }
    }
}

pub fn lookup_house(sender: u32, house_guid: u64) -> Result<House, ProcessPacketError> {
    info!("Found test house {}", house_guid);
    Ok(House {
        owner: sender,
        owner_name: "BLASTER NICESHOt".to_string(),
        custom_name: "Blaster's Test Lot".to_string(),
        rating: 3.5,
        total_votes: 100,
        fixtures: vec![
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
            },
            PreviousFixture {
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
                texture_alias: "".to_string(),
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

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Error},
    path::Path,
};

use byteorder::ReadBytesExt;
use packet_serialize::DeserializePacket;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use serde::Deserialize;

use crate::game_server::{
    packets::{
        client_update::{Stat, StatId, Stats},
        item::{BaseAttachmentGroup, WieldType},
        mount::{DismountReply, MountOpCode, MountReply, MountSpawn},
        player_update::{AddNpc, Icon, RemoveGracefully},
        tunnel::TunneledPacket,
        Effect, GamePacket, Pos,
    },
    Broadcast, GameServer, ProcessPacketError,
};

use super::{
    character::Character,
    guid::Guid,
    lock_enforcer::{CharacterLockRequest, ZoneLockRequest},
    unique_guid::{mount_guid, player_guid},
    zone::Zone,
};

#[derive(Deserialize)]
pub struct MountConfig {
    id: u32,
    speed_multiplier: f32,
    jump_height_multiplier: f32,
    gravity_multiplier: f32,
    model_id: u32,
    texture: String,
    pub name_id: u32,
    pub icon_set_id: u32,
    mount_composite_effect: u32,
    dismount_composite_effect: u32,
}

impl Guid<u32> for MountConfig {
    fn guid(&self) -> u32 {
        self.id
    }
}

pub fn load_mounts(config_dir: &Path) -> Result<BTreeMap<u32, MountConfig>, Error> {
    let mut file = File::open(config_dir.join("mounts.json"))?;
    let mounts: Vec<MountConfig> = serde_json::from_reader(&mut file)?;

    let mut mount_table = BTreeMap::new();
    for mount in mounts {
        let guid = mount.guid();
        let previous = mount_table.insert(guid, mount);

        if previous.is_some() {
            panic!("Two mounts have ID {}", guid);
        }
    }

    Ok(mount_table)
}

pub fn reply_dismount(
    sender: u32,
    zone: &RwLockReadGuard<Zone>,
    character: &mut RwLockWriteGuard<Character>,
    mounts: &BTreeMap<u32, MountConfig>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    if let Some(mount_id) = character.mount_id {
        character.mount_id = None;
        if let Some(mount) = mounts.get(&mount_id) {
            Ok(vec![Broadcast::Single(
                sender,
                vec![
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: DismountReply {
                            rider_guid: player_guid(sender),
                            composite_effect: mount.dismount_composite_effect,
                        },
                    })?,
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: RemoveGracefully {
                            guid: mount_guid(sender, mount_id),
                            unknown1: false,
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: 0,
                            timer: 1000,
                        },
                    })?,
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: Stats {
                            stats: vec![
                                Stat {
                                    id: StatId::Speed,
                                    multiplier: 1,
                                    value1: 0.0,
                                    value2: zone.speed,
                                },
                                Stat {
                                    id: StatId::JumpHeightMultiplier,
                                    multiplier: 1,
                                    value1: 0.0,
                                    value2: zone.jump_height_multiplier,
                                },
                                Stat {
                                    id: StatId::GravityMultiplier,
                                    multiplier: 1,
                                    value1: 0.0,
                                    value2: zone.gravity_multiplier,
                                },
                            ],
                        },
                    })?,
                ],
            )])
        } else {
            println!(
                "Player {} tried to dismount from non-existent mount",
                sender
            );
            Err(ProcessPacketError::CorruptedPacket)
        }
    } else {
        // Character is already dismounted
        Ok(Vec::new())
    }
}

fn process_dismount(
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![player_guid(sender)],
            character_consumer: |_, _, mut characters_write, zones_lock_enforcer| {
                if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender))
                {
                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                        read_guids: vec![character_write_handle.instance_guid],
                        write_guids: Vec::new(),
                        zone_consumer: |_, zones_read, _| {
                            if let Some(zone_read_handle) =
                                zones_read.get(&character_write_handle.instance_guid)
                            {
                                reply_dismount(
                                    sender,
                                    zone_read_handle,
                                    character_write_handle,
                                    game_server.mounts(),
                                )
                            } else {
                                println!("Player {} tried to enter unknown zone", sender);
                                Err(ProcessPacketError::CorruptedPacket)
                            }
                        },
                    })
                } else {
                    println!("Non-existent player {} tried to dismount", sender);
                    Err(ProcessPacketError::CorruptedPacket)
                }
            },
        })
}

fn process_mount_spawn(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let mount_spawn = MountSpawn::deserialize(cursor)?;
    let mount_guid = mount_guid(sender, mount_spawn.mount_id);

    if let Some(mount) = game_server.mounts().get(&mount_spawn.mount_id) {
        let packets = game_server.lock_enforcer().read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![player_guid(sender)],
            character_consumer: |_, _, mut characters_write, zones_lock_enforcer| {
                if let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender)) {
                    zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                        read_guids: vec![character_write_handle.instance_guid],
                        write_guids: Vec::new(),
                        zone_consumer: |_, zones_read, _| {
                            let mut packets = Vec::new();

                            if let Some(zone_read_handle) = zones_read.get(&character_write_handle.instance_guid) {
                                packets.append(&mut spawn_mount_npc(
                                    mount_guid,
                                    player_guid(sender),
                                    mount,
                                    character_write_handle.pos,
                                    character_write_handle.rot,
                                )?);

                                packets.push(GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: Stats {
                                        stats: vec![
                                            Stat {
                                                id: StatId::Speed,
                                                multiplier: 1,
                                                value1: 0.0,
                                                value2: zone_read_handle.speed * mount.speed_multiplier,
                                            },
                                            Stat {
                                                id: StatId::JumpHeightMultiplier,
                                                multiplier: 1,
                                                value1: 0.0,
                                                value2: zone_read_handle.jump_height_multiplier
                                                    * mount.jump_height_multiplier,
                                            },
                                            Stat {
                                                id: StatId::GravityMultiplier,
                                                multiplier: 1,
                                                value1: 0.0,
                                                value2: zone_read_handle.gravity_multiplier
                                                    * mount.gravity_multiplier,
                                            },
                                        ],
                                    },
                                })?);

                                if let Some(mount_id) = character_write_handle.mount_id {
                                    println!(
                                        "Player {} tried to mount while already mounted on mount ID {}",
                                        sender, mount_id
                                    );
                                    return Err(ProcessPacketError::CorruptedPacket);
                                }

                                character_write_handle.mount_id = Some(mount.guid());

                                Ok(packets)
                            } else {
                                println!("Player {} tried to mount but is in a non-existent zone", sender);
                                Err(ProcessPacketError::CorruptedPacket)
                            }
                        },
                    })
                } else {
                    println!("Non-existent player {} tried to mount", sender);
                    Err(ProcessPacketError::CorruptedPacket)
                }
            },
        })?;

        Ok(vec![Broadcast::Single(sender, packets)])
    } else {
        Err(ProcessPacketError::CorruptedPacket)
    }
}

pub fn process_mount_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code = cursor.read_u8()?;
    match MountOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            MountOpCode::DismountRequest => process_dismount(sender, game_server),
            MountOpCode::MountSpawn => process_mount_spawn(cursor, sender, game_server),
            _ => {
                println!("Unimplemented mount op code: {:?}", op_code);
                Ok(Vec::new())
            }
        },
        Err(_) => {
            println!("Unknown mount op code: {}", raw_op_code);
            Err(ProcessPacketError::CorruptedPacket)
        }
    }
}

pub fn spawn_mount_npc(
    mount_guid: u64,
    rider_guid: u64,
    mount: &MountConfig,
    spawn_pos: Pos,
    spawn_rot: Pos,
) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    Ok(vec![
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AddNpc {
                guid: mount_guid,
                name_id: mount.name_id,
                model_id: mount.model_id,
                unknown3: false,
                unknown4: 0,
                unknown5: 0,
                unknown6: 1,
                scale: 1.2,
                pos: spawn_pos,
                rot: spawn_rot,
                unknown8: 0,
                attachments: vec![],
                is_not_targetable: 1,
                unknown10: 0,
                texture_name: mount.texture.clone(),
                tint_name: "".to_string(),
                tint_id: 0,
                unknown11: true,
                offset_y: 0.0,
                composite_effect: 0,
                wield_type: WieldType::None,
                name_override: "".to_string(),
                hide_name: true,
                name_offset_x: 0.0,
                name_offset_y: 0.0,
                name_offset_z: 0.0,
                terrain_object_id: 0,
                invisible: false,
                unknown20: 0.0,
                unknown21: false,
                interactable_size_pct: 0,
                unknown23: -1,
                unknown24: -1,
                active_animation_slot: 1,
                unknown26: false,
                ignore_position: false,
                sub_title_id: 0,
                active_animation_slot2: 1,
                head_model_id: 0,
                effects: vec![Effect {
                    unknown1: 0,
                    unknown2: 0,
                    unknown3: 0,
                    unknown4: 0,
                    unknown5: 0,
                    unknown6: 0,
                    unknown7: 0,
                    unknown8: false,
                    unknown9: 0,
                    unknown10: 0,
                    unknown11: 0,
                    unknown12: 0,
                    composite_effect: mount.mount_composite_effect,
                    unknown14: 0,
                    unknown15: 0,
                    unknown16: 0,
                    unknown17: false,
                    unknown18: false,
                    unknown19: false,
                }],
                disable_interact_popup: true,
                unknown33: 0,
                unknown34: false,
                show_health: false,
                hide_despawn_fade: false,
                ignore_rotation_and_shadow: false,
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
                npc_type: 2,
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
                pet_customization_model_name1: "".to_string(),
                pet_customization_model_name2: "".to_string(),
                pet_customization_model_name3: "".to_string(),
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
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: MountReply {
                rider_guid,
                mount_guid,
                seat: 0,
                queue_pos: 1,
                unknown3: 1,
                composite_effect: 0,
                unknown5: 0,
            },
        })?,
    ])
}

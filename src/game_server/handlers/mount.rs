use std::{collections::BTreeMap, fs::File, io::Cursor, path::Path};

use packet_serialize::DeserializePacket;
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use serde::Deserialize;

use crate::{
    game_server::{
        packets::{
            client_update::{Stat, StatId, Stats},
            item::{BaseAttachmentGroup, WieldType},
            mount::{DismountReply, MountOpCode, MountReply, MountSpawn},
            player_update::{AddNpc, Hostility, Icon, PhysicsState, RemoveGracefully, UpdateSpeed},
            tunnel::TunneledPacket,
            Effect, GamePacket, Pos, Target,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    ConfigError,
};

use super::{
    character::{
        Character, CharacterLocationIndex, CharacterMatchmakingGroupIndex, CharacterMount,
        CharacterNameIndex, CharacterSquadIndex, CharacterSynchronizationIndex,
    },
    guid::{Guid, GuidTableIndexer, IndexedGuid},
    lock_enforcer::{CharacterLockRequest, ZoneLockEnforcer, ZoneLockRequest},
    unique_guid::{mount_guid, player_guid},
    zone::ZoneInstance,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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

pub fn load_mounts(config_dir: &Path) -> Result<BTreeMap<u32, MountConfig>, ConfigError> {
    let mut file = File::open(config_dir.join("mounts.yaml"))?;
    let mounts: Vec<MountConfig> = serde_yaml::from_reader(&mut file)?;

    let mut mount_table = BTreeMap::new();
    for mount in mounts {
        let guid = Guid::guid(&mount);
        let previous = mount_table.insert(guid, mount);

        if previous.is_some() {
            panic!("Two mounts have ID {guid}");
        }
    }

    Ok(mount_table)
}

pub fn reply_dismount<'a>(
    sender: u32,
    characters_table_handle: &'a impl GuidTableIndexer<
        'a,
        u64,
        Character,
        CharacterLocationIndex,
        CharacterNameIndex,
        CharacterSquadIndex,
        CharacterMatchmakingGroupIndex,
        CharacterSynchronizationIndex,
    >,
    zone: &RwLockReadGuard<ZoneInstance>,
    character: &mut RwLockWriteGuard<Character>,
    mounts: &BTreeMap<u32, MountConfig>,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let Some(CharacterMount {
        mount_id,
        mount_guid,
    }) = character.stats.mount
    else {
        // Character is already dismounted
        return Ok(Vec::new());
    };

    character.stats.mount = None;
    let Some(mount) = mounts.get(&mount_id) else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!("Player {sender} tried to dismount from non-existent mount"),
        ));
    };

    character.stats.speed.mount_multiplier = 1.0;
    character.stats.jump_height_multiplier.mount_multiplier = 1.0;
    let (_, instance_guid, chunk) = character.index1();
    let all_players_nearby =
        ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_handle);
    Ok(vec![
        Broadcast::Multi(
            all_players_nearby,
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: DismountReply {
                        rider_guid: player_guid(sender),
                        composite_effect: mount.dismount_composite_effect,
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: RemoveGracefully {
                        guid: mount_guid,
                        use_death_animation: false,
                        delay_millis: 0,
                        composite_effect_delay_millis: 0,
                        composite_effect: 0,
                        fade_duration_millis: 1000,
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: UpdateSpeed {
                        guid: player_guid(sender),
                        speed: character.stats.speed.total(),
                    },
                }),
            ],
        ),
        Broadcast::Single(
            sender,
            vec![GamePacket::serialize(&TunneledPacket {
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
            })],
        ),
    ])
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
            character_consumer: |characters_table_read_handle,
                                 _,
                                 mut characters_write,
                                 minigame_data_lock_enforcer| {
                let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender))
                else {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!("Non-existent player {sender} tried to dismount"),
                    ));
                };

                let zones_lock_enforcer: ZoneLockEnforcer = minigame_data_lock_enforcer.into();
                zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                    read_guids: vec![character_write_handle.stats.instance_guid],
                    write_guids: Vec::new(),
                    zone_consumer: |_, zones_read, _| {
                        let Some(zone_read_handle) =
                            zones_read.get(&character_write_handle.stats.instance_guid)
                        else {
                            return Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!("Player {sender} tried to enter unknown zone"),
                            ));
                        };

                        reply_dismount(
                            sender,
                            characters_table_read_handle,
                            zone_read_handle,
                            character_write_handle,
                            game_server.mounts(),
                        )
                    },
                })
            },
        })
}

fn process_mount_spawn(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let mount_spawn = MountSpawn::deserialize(cursor)?;
    let mount_guid = mount_guid(player_guid(sender));

    let Some(mount) = game_server.mounts().get(&mount_spawn.mount_id) else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "Player {sender} tried to mount on unknown mount ID {}",
                mount_spawn.mount_id
            ),
        ));
    };

    game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![player_guid(sender)],
            character_consumer: |characters_table_read_handle,
                                 _,
                                 mut characters_write,
                                 minigame_data_lock_enforcer| {
                let Some(character_write_handle) = characters_write.get_mut(&player_guid(sender))
                else {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!("Non-existent player {sender} tried to mount"),
                    ));
                };

                // TODO: check that player owns mount

                let zones_lock_enforcer: ZoneLockEnforcer = minigame_data_lock_enforcer.into();
                zones_lock_enforcer.read_zones(|_| ZoneLockRequest {
                    read_guids: vec![character_write_handle.stats.instance_guid],
                    write_guids: Vec::new(),
                    zone_consumer: |_, zones_read, _| {
                        let Some(zone_read_handle) =
                            zones_read.get(&character_write_handle.stats.instance_guid)
                        else {
                            return Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!(
                                    "Player {sender} tried to mount but is in a non-existent zone"
                                ),
                            ));
                        };

                        character_write_handle.stats.speed.mount_multiplier =
                            mount.speed_multiplier;
                        character_write_handle
                            .stats
                            .jump_height_multiplier
                            .mount_multiplier = mount.jump_height_multiplier;
                        let new_gravity =
                            zone_read_handle.gravity_multiplier * mount.gravity_multiplier;

                        let mut packets = spawn_mount_npc(
                            mount_guid,
                            player_guid(sender),
                            mount,
                            character_write_handle.stats.pos,
                            character_write_handle.stats.rot,
                            true,
                        );
                        packets.push(GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: UpdateSpeed {
                                guid: player_guid(sender),
                                speed: character_write_handle.stats.speed.total(),
                            },
                        }));

                        if let Some(CharacterMount { mount_id, mount_guid }) = character_write_handle.stats.mount {
                            return Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!(
                                    "Player {sender} tried to mount while already mounted on mount ID {mount_id}, GUID {mount_guid}"
                                ),
                            ));
                        }

                        character_write_handle.stats.mount = Some(CharacterMount { mount_id: Guid::guid(mount), mount_guid });

                        let (_, instance_guid, chunk) = character_write_handle.index1();
                        let all_players_nearby = ZoneInstance::all_players_nearby(
                            chunk,
                            instance_guid,
                            characters_table_read_handle,
                        );
                        Ok(vec![
                            Broadcast::Multi(all_players_nearby, packets),
                            Broadcast::Single(
                                sender,
                                vec![GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: Stats {
                                        stats: vec![
                                            Stat {
                                                id: StatId::Speed,
                                                multiplier: 1,
                                                value1: 0.0,
                                                value2: character_write_handle.stats.speed.total(),
                                            },
                                            Stat {
                                                id: StatId::JumpHeightMultiplier,
                                                multiplier: 1,
                                                value1: 0.0,
                                                value2: character_write_handle
                                                    .stats
                                                    .jump_height_multiplier
                                                    .total(),
                                            },
                                            Stat {
                                                id: StatId::GravityMultiplier,
                                                multiplier: 1,
                                                value1: 0.0,
                                                value2: new_gravity,
                                            },
                                        ],
                                    },
                                })],
                            ),
                        ])
                    },
                })
            },
        })
}

pub fn process_mount_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u8 = DeserializePacket::deserialize(cursor)?;
    match MountOpCode::try_from(raw_op_code) {
        Ok(op_code) => match op_code {
            MountOpCode::DismountRequest => process_dismount(sender, game_server),
            MountOpCode::MountSpawn => process_mount_spawn(cursor, sender, game_server),
            _ => Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!("Unimplemented mount op code: {op_code:?}"),
            )),
        },
        Err(_) => Err(ProcessPacketError::new(
            ProcessPacketErrorType::UnknownOpCode,
            format!("Unknown mount op code: {raw_op_code}"),
        )),
    }
}

pub fn spawn_mount_npc(
    mount_guid: u64,
    rider_guid: u64,
    mount: &MountConfig,
    spawn_pos: Pos,
    spawn_rot: Pos,
    show_spawn_effect: bool,
) -> Vec<Vec<u8>> {
    let effects = if show_spawn_effect {
        vec![Effect {
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
        }]
    } else {
        Vec::new()
    };
    vec![
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: AddNpc {
                guid: mount_guid,
                name_id: mount.name_id,
                model_id: mount.model_id,
                unknown3: false,
                chat_text_color: Character::DEFAULT_CHAT_TEXT_COLOR,
                chat_bubble_color: Character::DEFAULT_CHAT_BUBBLE_COLOR,
                chat_scale: 1,
                scale: 1.2,
                pos: spawn_pos,
                rot: spawn_rot,
                spawn_animation_id: 0,
                attachments: vec![],
                hostility: Hostility::Neutral,
                unknown10: 0,
                texture_alias: mount.texture.clone(),
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
                speed: 0.0,
                unknown21: false,
                interactable_size_pct: 0,
                walk_animation_id: -1,
                sprint_animation_id: -1,
                stand_animation_id: -1,
                unknown26: false,
                disable_gravity: false,
                sub_title_id: 0,
                one_shot_animation_id: -1,
                temporary_model: 0,
                effects,
                disable_interact_popup: true,
                unknown33: 0,
                unknown34: false,
                show_health: false,
                hide_despawn_fade: false,
                enable_tilt: false,
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
                collision: true,
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
        }),
    ]
}

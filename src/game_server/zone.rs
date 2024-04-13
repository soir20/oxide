use std::fs::File;
use std::io::Error;
use std::path::Path;

use parking_lot::RwLockReadGuard;
use serde::Deserialize;

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::{GameServer, ProcessPacketError};
use crate::game_server::client_update_packet::Position;
use crate::game_server::command::SelectPlayer;
use crate::game_server::game_packet::{GamePacket, OpCode, Pos};
use crate::game_server::guid::{Guid, GuidTable, GuidTableReadHandle, GuidTableWriteHandle};
use crate::game_server::login::{ClientBeginZoning, ZoneDetails};
use crate::game_server::player_update_packet::{AddNpc, BaseAttachmentGroup, Icon, NpcRelevance, SingleNpcRelevance, WeaponAnimation};
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::update_position::UpdatePlayerPosition;

#[derive(Deserialize)]
pub struct Door {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    terrain_object_id: u32,
    destination_pos_x: f32,
    destination_pos_y: f32,
    destination_pos_z: f32,
    destination_pos_w: f32,
    destination_rot_x: f32,
    destination_rot_y: f32,
    destination_rot_z: f32,
    destination_rot_w: f32,
    destination_zone: Option<u64>
}

#[derive(Deserialize)]
struct ZoneConfig {
    guid: u64,
    name: String,
    hide_ui: bool,
    direction_indicator: bool,
    spawn_pos_x: f32,
    spawn_pos_y: f32,
    spawn_pos_z: f32,
    spawn_pos_w: f32,
    spawn_rot_x: f32,
    spawn_rot_y: f32,
    spawn_rot_z: f32,
    spawn_rot_w: f32,
    spawn_sky: Option<String>,
    doors: Vec<Door>,
    door_interact_radius: f32,
    door_auto_interact_radius: f32
}

pub enum CharacterType {
    Door(Door),
    Player
}

pub struct Character {
    pub guid: u64,
    pub pos: Pos,
    pub rot: Pos,
    pub state: u8,
    pub character_type: CharacterType,
    pub interact_radius: f32,
    pub auto_interact_radius: f32
}

impl Guid for Character {
    fn guid(&self) -> u64 {
        self.guid
    }
}

impl Character {

    pub fn to_packets(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        let packets = match &self.character_type {
            CharacterType::Door(door) => {
                vec![
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: Self::door_packet(self, door),
                    })?,
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: NpcRelevance {
                            new_states: vec![
                                SingleNpcRelevance {
                                    guid: self.guid,
                                    cursor: Some(55),
                                    unknown1: false,
                                }
                            ],
                        },
                    })?
                ]
            },
            _ => Vec::new()
        };

        Ok(packets)
    }

    fn door_packet(character: &Character, door: &Door) -> AddNpc {
        AddNpc {
            guid: character.guid,
            name_id: 0,
            model_id: 0,
            unknown3: false,
            unknown4: 408679,
            unknown5: 13951728,
            unknown6: 1,
            scale: 1.0,
            pos: character.pos,
            rot: character.rot,
            unknown8: 1,
            attachments: vec![],
            is_terrain_object_noninteractable: 0,
            unknown10: 1,
            texture_name: "".to_string(),
            tint_name: "".to_string(),
            tint_id: 0,
            unknown11: true,
            offset_y: 0.0,
            composite_effect: 0,
            weapon_animation: WeaponAnimation::None,
            name_override: "".to_string(),
            hide_name: false,
            name_offset_x: 0.0,
            name_offset_y: 0.0,
            name_offset_z: 0.0,
            terrain_object_id: door.terrain_object_id,
            invisible: false,
            unknown20: 0.0,
            unknown21: false,
            interactable_size_pct: 100,
            unknown23: -1,
            unknown24: -1,
            active_animation_slot: -1,
            unknown26: false,
            ignore_position: false,
            sub_title_id: 0,
            active_animation_slot2: 0,
            head_model_id: 0,
            unknown31: vec![],
            disable_interact_popup: true,
            unknown33: 0,
            unknown34: false,
            show_health: false,
            unknown36: false,
            enable_move_to_interact: false,
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
            attachment_group_unknown: "".to_string(),
            unknown59: "".to_string(),
            unknown60: "".to_string(),
            override_terrain_model: false,
            hover_glow: 0,
            hover_description: 0,
            fly_over_effect: 0,
            unknown65: 8,
            unknown66: 0,
            unknown67: 3442,
            disable_move_to_interact: false,
            unknown69: 0.0,
            unknown70: 0.0,
            unknown71: 0,
            icon_id: Icon::None,
        }
    }
}

pub struct Zone {
    guid: u64,
    pub name: String,
    default_spawn_pos: Pos,
    default_spawn_rot: Pos,
    default_spawn_sky: String,
    hide_ui: bool,
    direction_indicator: bool,
    characters: GuidTable<Character>
}

impl Guid for Zone {
    fn guid(&self) -> u64 {
        self.guid
    }
}

impl Zone {
    pub fn send_self(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        Ok(vec![
            GamePacket::serialize(
                &TunneledPacket {
                    unknown1: true,
                    inner: ZoneDetails {
                        name: self.name.clone(),
                        zone_type: 2,
                        hide_ui: self.hide_ui,
                        direction_indicator: self.direction_indicator,
                        sky_definition_file_name: self.default_spawn_sky.clone(),
                        zoom_out: false,
                        unknown7: 0,
                        unknown8: 0,
                    },
                }
            )?
        ])
    }

    pub fn send_characters(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        let mut packets = Vec::new();
        for character in self.characters.read().values() {
            packets.append(&mut character.read().to_packets()?);
        }

        Ok(packets)
    }

    pub fn read_characters(&self) -> GuidTableReadHandle<Character> {
        self.characters.read()
    }

    pub fn write_characters(&self) -> GuidTableWriteHandle<Character> {
        self.characters.write()
    }

    pub fn move_character(&self, pos_update: UpdatePlayerPosition, game_server: &GameServer) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let characters = self.read_characters();
        let possible_character = characters.get(pos_update.guid);
        let mut characters_to_interact = Vec::new();

        if let Some(character) = possible_character {
            let mut write_handle = character.write();
            write_handle.pos = Pos {
                x: pos_update.pos_x,
                y: pos_update.pos_y,
                z: pos_update.pos_z,
                w: write_handle.pos.z,
            };
            write_handle.rot = Pos {
                x: pos_update.rot_x,
                y: pos_update.rot_y,
                z: pos_update.rot_z,
                w: write_handle.rot.z,
            };
            write_handle.state = pos_update.character_state;
            drop(write_handle);

            let read_handle = character.read();
            for character in characters.values() {
                let other_read_handle = character.read();
                if other_read_handle.auto_interact_radius > 0.0 {
                    let distance = distance3(
                        read_handle.pos.x,
                        read_handle.pos.y,
                        read_handle.pos.z,
                        other_read_handle.pos.x,
                        other_read_handle.pos.y,
                        other_read_handle.pos.z,
                    );
                    if distance <= other_read_handle.auto_interact_radius {
                        characters_to_interact.push(other_read_handle.guid);
                    }
                }
            }
        } else {
            println!("Received position update from unknown character {}", pos_update.guid);
            return Err(ProcessPacketError::CorruptedPacket);
        }

        drop(characters);

        let mut packets = Vec::new();
        for character_guid in characters_to_interact {
            let interact_request = SelectPlayer { requester: pos_update.guid, target: character_guid };
            packets.append(&mut interact_with_character(interact_request, game_server)?);
        }

        Ok(packets)
    }
}

impl From<ZoneConfig> for Zone {
    fn from(zone_config: ZoneConfig) -> Self {
        let characters = GuidTable::new();

        // Set the first bit for NPC guids to avoid player GUID conflicts
        let mut guid = 0x8000000000000000u64;

        {
            let mut write_handle = characters.write();
            for door in zone_config.doors {
                write_handle.insert(Character {
                    guid,
                    pos: Pos {
                        x: door.x,
                        y: door.y,
                        z: door.z,
                        w: door.w,
                    },
                    rot: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                    state: 0,
                    character_type: CharacterType::Door(door),
                    interact_radius: zone_config.door_interact_radius,
                    auto_interact_radius: zone_config.door_auto_interact_radius,
                });
                guid += 1;
            }
        }

        Zone {
            guid: zone_config.guid,
            name: zone_config.name,
            default_spawn_pos: Pos {
                x: zone_config.spawn_pos_x,
                y: zone_config.spawn_pos_y,
                z: zone_config.spawn_pos_z,
                w: zone_config.spawn_pos_w,
            },
            default_spawn_rot: Pos {
                x: zone_config.spawn_rot_x,
                y: zone_config.spawn_rot_y,
                z: zone_config.spawn_rot_z,
                w: zone_config.spawn_rot_w,
            },
            default_spawn_sky: zone_config.spawn_sky.unwrap_or("".to_string()),
            hide_ui: zone_config.hide_ui,
            direction_indicator: zone_config.direction_indicator,
            characters
        }
    }
}

pub fn load_zones(config_dir: &Path) -> Result<GuidTable<Zone>, Error> {
    let mut file = File::open(config_dir.join("zones.json"))?;
    let zone_configs: Vec<ZoneConfig> = serde_json::from_reader(&mut file)?;

    let zones = GuidTable::new();
    {
        let mut write_handle = zones.write();
        for zone_config in zone_configs {
            let zone = Zone::from(zone_config);
            let id = zone.guid;
            let previous = write_handle.insert(zone);

            if let Some(_) = previous {
                panic!("Two zones have ID {}", id);
            }
        }
    }

    Ok(zones)
}

pub fn interact_with_character(request: SelectPlayer, game_server: &GameServer) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    let zones = game_server.read_zones();
    if let Some(source_zone_guid) = GameServer::zone_with_player(&zones, request.requester) {

        if let Some(source_zone) = zones.get(source_zone_guid) {
            let source_zone_read_handle = source_zone.read();

            let characters = source_zone_read_handle.characters.read();
            let requester_x;
            let requester_y;
            let requester_z;
            if let Some(requester) = characters.get(request.requester) {
                let requester_read_handle = requester.read();
                requester_x = requester_read_handle.pos.x;
                requester_y = requester_read_handle.pos.y;
                requester_z = requester_read_handle.pos.z;
            } else {
                return Ok(Vec::new());
            }

            if let Some(target) = characters.get(request.target) {
                let target_read_handle = target.read();

                // Ensure the character is close enough to interact
                let distance = distance3(
                    requester_x,
                    requester_y,
                    requester_z,
                    target_read_handle.pos.x,
                    target_read_handle.pos.y,
                    target_read_handle.pos.z
                );
                if distance > target_read_handle.interact_radius {
                    return Ok(Vec::new());
                }

                // Process interaction based on character's type
                match &target_read_handle.character_type {
                    CharacterType::Door(door) => {
                        let destination_pos = Pos {
                            x: door.destination_pos_x,
                            y: door.destination_pos_y,
                            z: door.destination_pos_z,
                            w: door.destination_pos_w,
                        };
                        let destination_rot = Pos {
                            x: door.destination_rot_x,
                            y: door.destination_rot_y,
                            z: door.destination_rot_z,
                            w: door.destination_rot_w,
                        };

                        let destination_zone_guid = if let &Some(destination_zone_guid) = &door.destination_zone {
                            destination_zone_guid
                        } else {
                            source_zone_guid
                        };
                        drop(target_read_handle);
                        drop(characters);

                        if source_zone_guid != destination_zone_guid {
                            teleport_to_zone(
                                &zones,
                                source_zone_read_handle,
                                request.requester,
                                destination_zone_guid,
                                Some(destination_pos),
                                Some(destination_rot)
                            )
                        } else {
                            drop(source_zone_read_handle);
                            teleport_within_zone(destination_pos, destination_rot)
                        }
                    },
                    _ => Ok(Vec::new())
                }

            } else {
                println!("Received request to interact with unknown NPC {} from {}", request.target, request.requester);
                Err(ProcessPacketError::CorruptedPacket)
            }

        } else {
            println!("Zone {} was destroyed before interaction could be processed", source_zone_guid);
            Ok(vec![])
        }

    } else {
        println!("Requested interaction from unknown player {}", request.requester);
        Err(ProcessPacketError::CorruptedPacket)
    }
}

pub fn teleport_within_zone(destination_pos: Pos, destination_rot: Pos) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    Ok(
        vec![
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: Position {
                    player_pos: destination_pos,
                    rot: destination_rot,
                    is_teleport: true,
                    unknown2: true,
                },
            })?
        ]
    )
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ZoneTeleportRequest {
    pub destination_guid: u32
}

impl GamePacket for ZoneTeleportRequest {
    type Header = OpCode;
    const HEADER: Self::Header = OpCode::ZoneTeleportRequest;
}

pub fn teleport_to_zone(zones: &GuidTableReadHandle<Zone>, source_zone: RwLockReadGuard<Zone>,
                        player_guid: u64, destination_zone_guid: u64, destination_pos: Option<Pos>,
                        destination_rot: Option<Pos>) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    let mut characters = source_zone.write_characters();
    let character = characters.remove(player_guid);
    drop(characters);
    drop(source_zone);

    if let Some(destination_zone) = zones.get(destination_zone_guid) {
        let destination_read_handle = destination_zone.read();
        let destination_pos = destination_pos.unwrap_or(destination_read_handle.default_spawn_pos);
        let destination_rot = destination_rot.unwrap_or(destination_read_handle.default_spawn_rot);
        if let Some(character) = character {
            let mut characters = destination_read_handle.write_characters();
            characters.insert_lock(player_guid, character);
            drop(characters);
        }
        Ok(prepare_init_zone_packets(destination_read_handle, destination_pos, destination_rot)?)
    } else {
        Ok(Vec::new())
    }
}


fn prepare_init_zone_packets(destination: RwLockReadGuard<Zone>, destination_pos: Pos,
                             destination_rot: Pos) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    let zone_name = destination.name.clone();
    let mut packets = vec![];
    packets.push(
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: ClientBeginZoning {
                zone_name,
                zone_type: 2,
                pos: destination_pos,
                rot: destination_rot,
                sky_definition_file_name: destination.default_spawn_sky.clone(),
                unknown1: false,
                zone_id: 0,
                zone_name_id: 0,
                world_id: 0,
                world_name_id: 0,
                unknown6: false,
                unknown7: false,
            }
        })?
    );

    Ok(packets)
}

fn distance3(x1: f32, y1: f32, z1: f32, x2: f32, y2: f32, z2: f32) -> f32 {
    let diff_x = x2 - x1;
    let diff_y = y2 - y1;
    let diff_z = z2 - z1;
    (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt()
}

use std::fs::File;
use std::io::Error;
use std::path::Path;

use serde::Deserialize;

use packet_serialize::SerializePacketError;

use crate::game_server::client_update_packet::Position;
use crate::game_server::command::SelectPlayer;
use crate::game_server::game_packet::{GamePacket, Pos};
use crate::game_server::guid::{Guid, GuidTable, GuidTableReadHandle, GuidTableWriteHandle};
use crate::game_server::login::{ClientBeginZoning, ZoneDetails};
use crate::game_server::player_update_packet::{AddNotifications, AddNpc, BaseAttachmentGroup, DamageAnimation, HoverGlow, Icon, NotificationData, NpcRelevance, SingleNotification, SingleNpcRelevance, WeaponAnimation};
use crate::game_server::tunnel::TunneledPacket;

#[derive(Deserialize)]
pub struct Door {
    terrain_object_id: u32,
    destination_pos_x: f32,
    destination_pos_y: f32,
    destination_pos_z: f32,
    destination_pos_w: f32,
    destination_rot_x: f32,
    destination_rot_y: f32,
    destination_rot_z: f32,
    destination_rot_w: f32,
    destination_zone: Option<String>
}

#[derive(Deserialize)]
struct ZoneConfig {
    guid: u64,
    name: String,
    hide_ui: bool,
    direction_indicator: bool,
    doors: Vec<Door>
}

pub enum CharacterType {
    Door(Door),
    Player
}

pub struct Character {
    pub guid: u64,
    pub pos: Pos,
    pub rot: Pos,
    pub character_type: CharacterType
}

impl Guid for Character {
    fn guid(&self) -> u64 {
        self.guid
    }
}

impl Character {
    pub fn interact(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        match &self.character_type {
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

                let teleport_packet = if let Some(destination_zone) = &door.destination_zone {
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: ClientBeginZoning {
                            zone_name: destination_zone.clone(),
                            zone_type: 2,
                            pos: destination_pos,
                            rot: destination_rot,
                            sky_definition_file_name: "".to_string(),
                            unknown1: false,
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: 0,
                            unknown5: 0,
                            unknown6: false,
                            unknown7: false,
                        }
                    })?
                } else {
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: Position {
                            player_pos: destination_pos,
                            rot: destination_rot,
                            is_teleport: true,
                            unknown2: true,
                        },
                    })?
                };

                Ok(vec![teleport_packet])
            },
            _ => Ok(Vec::new())
        }
    }

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
                    })?,
                    GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: AddNotifications {
                            notifications: vec![
                                SingleNotification {
                                    guid: self.guid,
                                    unknown1: 1,
                                    notification: Some(
                                        NotificationData {
                                            unknown1: 1,
                                            icon_id: 13,
                                            unknown3: 0,
                                            name_id: 0,
                                            unknown4: 0,
                                            hide_icon: false,
                                            unknown6: 0,
                                        }
                                    ),
                                    unknown2: true,
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
            damage_animation: DamageAnimation::None,
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
            unknown32: false,
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
            unknown42: 2639,
            collision: true,
            unknown44: 0,
            unknown45: 2,
            unknown46: 0.0,
            target: 0,
            unknown50: vec![],
            trick_animation_id: 0,
            unknown52: 0.0,
            unknown53: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 0.0,
            },
            unknown54: 0,
            unknown55: 0.0,
            unknown56: 0.0,
            unknown57: 0.0,
            attachment_group_unknown: "".to_string(),
            unknown59: "".to_string(),
            unknown60: "".to_string(),
            force_load_actor_definition: false,
            hover_glow: HoverGlow::Disabled,
            unknown63: 0,
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
    name: String,
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
                        sky_definition_file_name: "".to_string(),
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

    pub fn interact_with_character(&self, request: SelectPlayer) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        if let Some(target) = self.characters.read().get(request.target) {
            target.read().interact()
        } else {
            println!("Received request to interact with unknown NPC {} from {}", request.target, request.requester);
            Ok(vec![])
        }
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
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 1.0,
                    },
                    rot: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                    character_type: CharacterType::Door(door),
                });
                guid += 1;
            }
        }

        Zone {
            guid: zone_config.guid,
            name: zone_config.name,
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

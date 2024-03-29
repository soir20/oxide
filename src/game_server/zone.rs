use std::fs::File;
use std::io::Error;
use std::path::Path;

use serde::Deserialize;

use packet_serialize::SerializePacketError;

use crate::game_server::client_update_packet::Position;
use crate::game_server::command::InteractionRequest;
use crate::game_server::game_packet::{GamePacket, Pos};
use crate::game_server::guid::{Guid, GuidTable, GuidTableReadHandle, GuidTableWriteHandle};
use crate::game_server::login::ZoneDetails;
use crate::game_server::player_data::PlayerState;
use crate::game_server::player_update_packet::{AddNpc, DamageAnimation, HoverGlow, Icon, MoveAnimation, Unknown, WeaponAnimation};
use crate::game_server::tunnel::TunneledPacket;

#[derive(Deserialize)]
struct Door {
    terrain_object_id: u32,
    destination_pos_x: f32,
    destination_pos_y: f32,
    destination_pos_z: f32,
    destination_rot: f32,
    destination_camera_x: f32,
    destination_camera_y: f32,
    destination_camera_z: f32,
    destination_camera_rot: f32,
}

#[derive(Deserialize)]
struct ZoneConfig {
    guid: u64,
    name: String,
    hide_ui: bool,
    direction_indicator: bool,
    doors: Vec<Door>
}

enum Npc {
    Door(u64, Door)
}

impl Guid for Npc {
    fn guid(&self) -> u64 {
        match self {
            Npc::Door(guid, _) => *guid
        }
    }
}

impl Npc {
    pub fn interact(&mut self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        match self {
            Npc::Door(_, door) => {
                let pos_update = TunneledPacket {
                    unknown1: true,
                    inner: Position {
                        player_pos: Pos {
                            x: door.destination_pos_x,
                            y: door.destination_pos_y,
                            z: door.destination_pos_z,
                            rot: door.destination_rot,
                        },
                        camera_pos: Pos {
                            x: door.destination_camera_x,
                            y: door.destination_camera_y,
                            z: door.destination_camera_z,
                            rot: door.destination_camera_rot,
                        },
                        unknown1: true,
                        unknown2: true,
                    },
                };
                Ok(
                    vec![
                        pos_update.serialize()?
                    ]
                )
            }
        }
    }

    pub fn to_packet(&self) -> Result<Vec<u8>, SerializePacketError> {
        Ok(
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: match self {
                    Npc::Door(guid, door) => {
                        Self::door_packet(*guid, door)
                    }
                },
            })?
        )
    }

    fn door_packet(guid: u64, door: &Door) -> AddNpc {
        AddNpc {
            guid,
            name_id: 0,
            model_id: 0,
            unknown3: false,
            unknown4: 0,
            unknown5: 0,
            unknown6: 1,
            scale: 1.0,
            position: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                rot: 1.0,
            },
            rotation: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                rot: 1.0,
            },
            unknown8: 0,
            attachments: vec![],
            is_terrain_object_noninteractable: 0,
            unknown10: 0,
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
            move_animation: MoveAnimation::Standing,
            unknown26: false,
            unknown27: false,
            sub_title_id: 0,
            move_animation2: MoveAnimation::Standing,
            head_model_id: 0,
            unknown31: vec![],
            unknown32: false,
            unknown33: 0,
            unknown34: false,
            show_health: false,
            unknown36: false,
            enable_move_to_interact: false,
            unknown38: Unknown {
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
                rot: 0.0,
            },
            unknown40: 0,
            unknown41: -1,
            unknown42: 0,
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
                rot: 0.0,
            },
            unknown54: 0,
            unknown55: 0.0,
            unknown56: 0.0,
            unknown57: 0.0,
            unknown58: "".to_string(),
            unknown59: "".to_string(),
            unknown60: "".to_string(),
            is_not_terrain_object: false,
            hover_glow: HoverGlow::Enabled,
            unknown63: 0,
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
}

pub struct Zone {
    guid: u64,
    name: String,
    hide_ui: bool,
    direction_indicator: bool,
    npcs: GuidTable<Npc>,
    players: GuidTable<PlayerState>
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

    pub fn send_npcs(&self) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        let mut packets = Vec::new();
        for npc in self.npcs.read().values() {
            packets.push(npc.read().to_packet()?);
        }

        Ok(packets)
    }

    pub fn read_players(&self) -> GuidTableReadHandle<PlayerState> {
        self.players.read()
    }

    pub fn write_players(&self) -> GuidTableWriteHandle<PlayerState> {
        self.players.write()
    }

    pub fn interact_npc(&self, request: InteractionRequest) -> Result<Vec<Vec<u8>>, SerializePacketError> {
        if let Some(npc) = self.npcs.read().get(request.target) {
            npc.write().interact()
        } else {
            println!("Received request to interact with unknown NPC {} from {}", request.target, request.requester);
            Ok(vec![])
        }
    }
}

impl From<ZoneConfig> for Zone {
    fn from(zone_config: ZoneConfig) -> Self {
        let npcs = GuidTable::new();

        // Use the upper half of the GUID for NPC guids to avoid player GUID conflicts
        let mut guid = 0xFFFFFFFF00000000u64;

        {
            let mut write_handle = npcs.write();
            for door in zone_config.doors {
                write_handle.insert(Npc::Door(guid, door));
                guid += 1;
            }
        }

        Zone {
            guid: zone_config.guid,
            name: zone_config.name,
            hide_ui: zone_config.hide_ui,
            direction_indicator: zone_config.direction_indicator,
            npcs,
            players: GuidTable::new()
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

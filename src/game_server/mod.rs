use std::io::{Cursor, Error};
use std::path::Path;
use std::vec;

use byteorder::{LittleEndian, ReadBytesExt};

use packet_serialize::{DeserializePacket, DeserializePacketError, NullTerminatedString, SerializePacketError};

use crate::game_server::client_update_packet::{Health, Power, PreloadCharactersDone, Stat, StatId, Stats};
use crate::game_server::command::process_command;
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::guid::{Guid, GuidTable, GuidTableReadHandle, GuidTableWriteHandle};
use crate::game_server::housing::make_test_fixture_packets;
use crate::game_server::item::make_item_definitions;
use crate::game_server::login::{DeploymentEnv, GameSettings, LoginReply, send_points_of_interest, WelcomeScreen, ZoneDetailsDone};
use crate::game_server::mount::{process_mount_packet, load_mounts, MountConfig};
use crate::game_server::player_data::{make_test_wield_type, make_test_player};
use crate::game_server::player_update_packet::make_test_npc;
use crate::game_server::time::make_game_time_sync;
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::update_position::UpdatePlayerPosition;
use crate::game_server::zone::{Character, load_zones, teleport_to_zone, teleport_within_zone, Zone, ZoneTeleportRequest};

mod login;
mod player_data;
mod tunnel;
mod game_packet;
mod time;
mod client_update_packet;
mod player_update_packet;
mod command;
mod zone;
mod guid;
mod update_position;
mod ui;
mod combat_update_packet;
mod item;
mod store;
mod mount;
mod housing;

#[derive(Debug)]
pub enum Broadcast {
    Single(u32, Vec<Vec<u8>>),
    Multi(Vec<u32>, Vec<Vec<u8>>)
}

#[non_exhaustive]
#[derive(Debug)]
pub enum ProcessPacketError {
    CorruptedPacket,
    SerializeError(SerializePacketError)
}

impl From<Error> for ProcessPacketError {
    fn from(_: Error) -> Self {
        ProcessPacketError::CorruptedPacket
    }
}

impl From<DeserializePacketError> for ProcessPacketError {
    fn from(_: DeserializePacketError) -> Self {
        ProcessPacketError::CorruptedPacket
    }
}

impl From<SerializePacketError> for ProcessPacketError {
    fn from(value: SerializePacketError) -> Self {
        ProcessPacketError::SerializeError(value)
    }
}

pub struct GameServer {
    mounts: GuidTable<u32, MountConfig>,
    zones: GuidTable<u32, Zone>,
}

impl GameServer {

    pub fn new(config_dir: &Path) -> Result<Self, Error> {
        Ok(
            GameServer {
                mounts: load_mounts(config_dir)?,
                zones: load_zones(config_dir)?,
            }
        )
    }

    pub fn login(&self, data: Vec<u8>) -> Result<(u32, Vec<Broadcast>), ProcessPacketError> {
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::LoginRequest => {

                    // TODO: validate and get GUID from login request
                    let guid = 1;

                    // TODO: get player's zone
                    let player_zone = 24;

                    let mut packets = Vec::new();

                    let login_reply = TunneledPacket {
                        unknown1: true,
                        inner: LoginReply {
                            logged_in: true,
                        },
                    };
                    packets.push(GamePacket::serialize(&login_reply)?);

                    let deployment_env = TunneledPacket {
                        unknown1: true,
                        inner: DeploymentEnv {
                            environment: NullTerminatedString("prod".to_string()),
                        },
                    };
                    packets.push(GamePacket::serialize(&deployment_env)?);
                    let mut zone_details = self.zones.read().get(player_zone).unwrap().read().send_self()?;
                    packets.append(&mut zone_details);

                    let settings = TunneledPacket {
                        unknown1: true,
                        inner: GameSettings {
                            unknown1: 4,
                            unknown2: 7,
                            unknown3: 268,
                            unknown4: true,
                            time_scale: 1.0,
                        },
                    };
                    packets.push(GamePacket::serialize(&settings)?);

                    let item_defs = TunneledPacket {
                        unknown1: true,
                        inner: make_item_definitions()
                    };
                    packets.push(GamePacket::serialize(&item_defs)?);

                    let player = TunneledPacket {
                        unknown1: true,
                        inner: make_test_player(guid, &self.mounts.read())
                    };
                    packets.push(GamePacket::serialize(&player)?);

                    if let Some(zone) = self.zones.read().get(player_zone) {
                        zone.read().write_characters().insert(
                            Character::from(player.inner.data)
                        );
                    } else {
                        return Err(ProcessPacketError::CorruptedPacket);
                    }

                    Ok((guid, vec![Broadcast::Single(guid, packets)]))
                },
                _ => {
                    println!("Client tried to log in without a login request");
                    Err(ProcessPacketError::CorruptedPacket)
                }
            },
            Err(_) => {
                println!("Unknown op code at login: {}", raw_op_code);
                Err(ProcessPacketError::CorruptedPacket)
            }
        }
    }
    
    pub fn process_packet(&self, sender: u32, data: Vec<u8>) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let mut broadcasts = Vec::new();
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::TunneledClient => {
                    let packet: TunneledPacket<Vec<u8>> = DeserializePacket::deserialize(&mut cursor)?;
                    broadcasts.append(&mut self.process_packet(sender, packet.inner)?);
                },
                OpCode::ClientIsReady => {
                    let mut packets = Vec::new();

                    packets.append(&mut send_points_of_interest(self)?);

                    let npc = TunneledPacket {
                        unknown1: true,
                        inner: make_test_npc()
                    };
                    //packets.push(GamePacket::serialize(&npc)?);

                    let zones = self.read_zones();
                    if let Some(zone) = GameServer::zone_with_character(&zones, sender as u64) {
                        let zone_read_handle = zones.get(zone).unwrap().read();
                        let mut preloaded_npcs = zone_read_handle.send_characters()?;
                        packets.append(&mut preloaded_npcs);

                        let stats = TunneledPacket {
                            unknown1: true,
                            inner: Stats {
                                stats: vec![
                                    Stat {
                                        id: StatId::Speed,
                                        multiplier: 1,
                                        value1: 0.0,
                                        value2: zone_read_handle.speed,
                                    },
                                    Stat {
                                        id: StatId::PowerRegen,
                                        multiplier: 1,
                                        value1: 0.0,
                                        value2: 1.0,
                                    },
                                    Stat {
                                        id: StatId::PowerRegen,
                                        multiplier: 1,
                                        value1: 0.0,
                                        value2: 1.0,
                                    },
                                    Stat {
                                        id: StatId::GravityMultiplier,
                                        multiplier: 1,
                                        value1: 0.0,
                                        value2: zone_read_handle.gravity_multiplier,
                                    },
                                    Stat {
                                        id: StatId::JumpHeightMultiplier,
                                        multiplier: 1,
                                        value1: 0.0,
                                        value2: zone_read_handle.jump_height_multiplier,
                                    },

                                ],
                            },
                        };
                        packets.push(GamePacket::serialize(&stats)?);
                    }

                    let health = TunneledPacket {
                        unknown1: true,
                        inner: Health {
                            current: 25000,
                            max: 25000,
                        },
                    };
                    packets.push(GamePacket::serialize(&health)?);

                    let power = TunneledPacket {
                        unknown1: true,
                        inner: Power {
                            current: 300,
                            max: 300,
                        },
                    };
                    packets.push(GamePacket::serialize(&power)?);

                    packets.append(&mut make_test_wield_type(sender)?);

                    let welcome_screen = TunneledPacket {
                        unknown1: true,
                        inner: WelcomeScreen {
                            show_ui: true,
                            unknown1: vec![],
                            unknown2: vec![],
                            unknown3: 0,
                            unknown4: 0,
                        },
                    };
                    packets.push(GamePacket::serialize(&welcome_screen)?);

                    let zone_details_done = TunneledPacket {
                        unknown1: true,
                        inner: ZoneDetailsDone {},
                    };
                    packets.push(GamePacket::serialize(&zone_details_done)?);

                    let preload_characters_done = TunneledPacket {
                        unknown1: true,
                        inner: PreloadCharactersDone {
                            unknown1: false
                        },
                    };
                    packets.push(GamePacket::serialize(&preload_characters_done)?);

                    packets.append(&mut make_test_fixture_packets()?);

                    broadcasts.push(Broadcast::Single(sender, packets));
                },
                OpCode::GameTimeSync => {
                    let game_time_sync = TunneledPacket {
                        unknown1: true,
                        inner: make_game_time_sync(),
                    };
                    broadcasts.push(Broadcast::Single(sender, vec![GamePacket::serialize(&game_time_sync)?]));
                },
                OpCode::Command => {
                    broadcasts.push(Broadcast::Single(sender, process_command(self, &mut cursor)?));
                },
                OpCode::UpdatePlayerPosition => {
                    let pos_update: UpdatePlayerPosition = DeserializePacket::deserialize(&mut cursor)?;
                    let zones = self.read_zones();
                    if let Some(zone_guid) = GameServer::zone_with_character(&zones, sender as u64) {
                        let zone = zones.get(zone_guid).unwrap().read();

                        // TODO: broadcast pos update to all players
                        broadcasts.push(Broadcast::Single(sender, zone.move_character(pos_update, self)?));

                    }
                },
                OpCode::ZoneTeleportRequest => {
                    let mut packets = Vec::new();
                    let teleport_request: ZoneTeleportRequest = DeserializePacket::deserialize(&mut cursor)?;

                    let zones = self.read_zones();
                    if let Some(zone_guid) = GameServer::zone_with_character(&zones, sender as u64) {
                        if let Some(zone) = zones.get(zone_guid) {
                            let zone_read_handle = zone.read();
                            packets.append(
                                &mut teleport_to_zone(
                                    &zones,
                                    zone_read_handle,
                                    sender as u64,
                                    teleport_request.destination_guid,
                                    None,
                                    None
                                )?
                            );
                        }
                    } else {
                        println!("Received teleport request for player not in any zone");
                    }

                    broadcasts.push(Broadcast::Single(sender, packets));
                },
                OpCode::TeleportToSafety => {
                    let mut packets = Vec::new();

                    let zones = self.read_zones();
                    if let Some(zone_guid) = GameServer::zone_with_character(&zones, sender as u64) {
                        if let Some(zone) = zones.get(zone_guid) {
                            let zone_read_handle = zone.read();

                            let spawn_pos = zone_read_handle.default_spawn_pos;
                            let spawn_rot = zone_read_handle.default_spawn_rot;
                            drop(zone_read_handle);

                            packets.append(&mut teleport_within_zone(
                                spawn_pos,
                                spawn_rot
                            )?);
                        }
                    } else {
                        println!("Received teleport to safety request for player not in any zone");
                    }

                    broadcasts.push(Broadcast::Single(sender, packets));
                },
                OpCode::Mount => {
                    broadcasts.append(&mut process_mount_packet(&mut cursor, sender, &self)?);
                },
                _ => println!("Unimplemented: {:?}, {:x?}", op_code, data)
            },
            Err(_) => println!("Unknown op code: {}, {:x?}", raw_op_code, data)
        }

        Ok(broadcasts)
    }

    pub fn read_zones(&self) -> GuidTableReadHandle<u32, Zone> {
        self.zones.read()
    }

    pub fn write_zones(&self) -> GuidTableWriteHandle<u32, Zone> {
        self.zones.write()
    }

    pub fn zone_with_character(zones: &GuidTableReadHandle<u32, Zone>, guid: u64) -> Option<u32> {
        for zone in zones.values() {
            let read_handle = zone.read();
            if read_handle.read_characters().get(guid).is_some() {
                return Some(read_handle.guid());
            }
        }

        None
    }
    
}

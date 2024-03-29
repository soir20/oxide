use std::io::{Cursor, Error};
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt};

use packet_serialize::{DeserializePacket, DeserializePacketError, NullTerminatedString, SerializePacketError};

use crate::game_server::client_update_packet::{Health, Power, PreloadCharactersDone, Stat, Stats};
use crate::game_server::command::process_command;
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::guid::{Guid, GuidTable, GuidTableReadHandle, GuidTableWriteHandle};
use crate::game_server::login::{DeploymentEnv, GameSettings, LoginReply, WelcomeScreen, ZoneDetailsDone};
use crate::game_server::player_data::{make_test_player, PlayerState};
use crate::game_server::player_update_packet::make_test_npc;
use crate::game_server::time::make_game_time_sync;
use crate::game_server::tunnel::TunneledPacket;
use crate::game_server::zone::{load_zones, Zone};

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
    zones: GuidTable<Zone>
}

impl GameServer {

    pub fn new(config_dir: &Path) -> Result<Self, Error> {
        Ok(
            GameServer {
                zones: load_zones(config_dir)?,
            }
        )
    }
    
    pub fn process_packet(&mut self, data: Vec<u8>) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
        let mut result_packets = Vec::new();
        let mut cursor = Cursor::new(&data[..]);
        let raw_op_code = cursor.read_u16::<LittleEndian>()?;

        match OpCode::try_from(raw_op_code) {
            Ok(op_code) => match op_code {
                OpCode::LoginRequest => {
                    let login_reply = TunneledPacket {
                        unknown1: true,
                        inner: LoginReply {
                            logged_in: true,
                        },
                    };
                    result_packets.push(GamePacket::serialize(&login_reply)?);

                    let deployment_env = TunneledPacket {
                        unknown1: true,
                        inner: DeploymentEnv {
                            environment: NullTerminatedString("prod".to_string()),
                        },
                    };
                    result_packets.push(GamePacket::serialize(&deployment_env)?);

                    // TODO: get player's zone
                    let mut zone_details = self.zones.read().get(2).unwrap().read().send_self()?;
                    result_packets.append(&mut zone_details);

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
                    result_packets.push(GamePacket::serialize(&settings)?);

                    let player = TunneledPacket {
                        unknown1: true,
                        inner: make_test_player()
                    };
                    result_packets.push(GamePacket::serialize(&player)?);

                    // TODO: get player's zone
                    self.zones.read().get(2).unwrap().read().write_players().insert(PlayerState::from(player.inner.data));

                },
                OpCode::TunneledClient => {
                    let packet: TunneledPacket<Vec<u8>> = DeserializePacket::deserialize(&mut cursor)?;
                    result_packets.append(&mut self.process_packet(packet.inner)?);
                },
                OpCode::ClientIsReady => {
                    let npc = TunneledPacket {
                        unknown1: true,
                        inner: make_test_npc()
                    };
                    //result_packets.push(GamePacket::serialize(&npc)?);

                    // TODO: get player's zone
                    let mut preloaded_npcs = self.zones.read().get(2).unwrap().read().send_npcs()?;
                    result_packets.append(&mut preloaded_npcs);

                    let health = TunneledPacket {
                        unknown1: true,
                        inner: Health {
                            unknown1: 25000,
                            unknown2: 25000,
                        },
                    };
                    result_packets.push(GamePacket::serialize(&health)?);

                    let power = TunneledPacket {
                        unknown1: true,
                        inner: Power {
                            unknown1: 300,
                            unknown2: 300,
                        },
                    };
                    result_packets.push(GamePacket::serialize(&power)?);

                    let stats = TunneledPacket {
                        unknown1: true,
                        inner: Stats {
                            stats: vec![

                                // Movement speed
                                Stat {
                                    id1: 2,
                                    id2: 1,
                                    value1: 0.0,
                                    value2: 8.0,
                                },

                                // Health refill
                                Stat {
                                    id1: 4,
                                    id2: 0,
                                    value1: 0.0,
                                    value2: 1.0,
                                },

                                // Power refill
                                Stat {
                                    id1: 6,
                                    id2: 0,
                                    value1: 0.0,
                                    value2: 1.0,
                                },

                                // Extra gravity
                                Stat {
                                    id1: 58,
                                    id2: 0,
                                    value1: 0.0,
                                    value2: 0.0,
                                },

                                // Extra jump height
                                Stat {
                                    id1: 59,
                                    id2: 0,
                                    value1: 0.0,
                                    value2: 0.0,
                                },

                            ],
                        },
                    };
                    result_packets.push(GamePacket::serialize(&stats)?);

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
                    result_packets.push(GamePacket::serialize(&welcome_screen)?);

                    let zone_details_done = TunneledPacket {
                        unknown1: true,
                        inner: ZoneDetailsDone {},
                    };
                    result_packets.push(GamePacket::serialize(&zone_details_done)?);

                    let preload_characters_done = TunneledPacket {
                        unknown1: true,
                        inner: PreloadCharactersDone {
                            unknown1: false
                        },
                    };
                    result_packets.push(GamePacket::serialize(&preload_characters_done)?);
                },
                OpCode::GameTimeSync => {
                    let game_time_sync = TunneledPacket {
                        unknown1: true,
                        inner: make_game_time_sync(),
                    };
                    result_packets.push(GamePacket::serialize(&game_time_sync)?);
                },
                OpCode::Command => {
                    let mut result_commands = process_command(self, &mut cursor)?;
                    result_packets.append(&mut result_commands);
                },
                _ => println!("Unimplemented: {:?}", op_code)
            },
            Err(_) => println!("Unknown op code: {}", raw_op_code)
        }

        Ok(result_packets)
    }

    pub fn read_zones(&self) -> GuidTableReadHandle<Zone> {
        self.zones.read()
    }

    pub fn write_zones(&self) -> GuidTableWriteHandle<Zone> {
        self.zones.write()
    }

    pub fn zone_with_player(zones: &GuidTableReadHandle<Zone>, guid: u64) -> Option<u64> {
        for zone in zones.values() {
            let read_handle = zone.read();
            if read_handle.read_players().get(guid).is_some() {
                return Some(read_handle.guid());
            }
        }

        None
    }
    
}

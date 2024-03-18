use std::io::{Cursor, Error};
use byteorder::{LittleEndian, ReadBytesExt};
use packet_serialize::{DeserializePacket, DeserializePacketError, NullTerminatedString, SerializePacketError};
use crate::game_server::client_update_packet::{Health, Power, PreloadCharactersDone, Stat, Stats};
use crate::game_server::login::{DeploymentEnv, GameSettings, LoginReply, WelcomeScreen, ZoneDetails, ZoneDetailsDone};
use crate::game_server::game_packet::{GamePacket, OpCode};
use crate::game_server::player_data::make_test_player;
use crate::game_server::time::make_game_time_sync;
use crate::game_server::tunnel::TunneledPacket;

mod login;
mod player_data;
mod tunnel;
mod game_packet;
mod time;
mod client_update_packet;

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
    
}

impl GameServer {
    
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

                    let zone_details = TunneledPacket {
                        unknown1: true,
                        inner: ZoneDetails {
                            name: "JediTemple".to_string(),
                            id: 2,
                            unknown2: false,
                            unknown3: false,
                            unknown5: "".to_string(),
                            unknown6: false,
                            unknown7: 0,
                            unknown8: 5,
                        },
                    };
                    result_packets.push(GamePacket::serialize(&zone_details)?);

                    let settings = TunneledPacket {
                        unknown1: true,
                        inner: GameSettings {
                            unknown1: 0,
                            unknown2: 0,
                            unknown3: 0,
                            unknown4: true,
                            unknown5: 1.0,
                        },
                    };
                    result_packets.push(GamePacket::serialize(&settings)?);

                    let player = TunneledPacket {
                        unknown1: true,
                        inner: make_test_player()
                    };
                    result_packets.push(GamePacket::serialize(&player)?);
                },
                OpCode::TunneledClient => {
                    let packet: TunneledPacket<Vec<u8>> = DeserializePacket::deserialize(&mut cursor)?;
                    result_packets.append(&mut self.process_packet(packet.inner)?);
                },
                OpCode::ClientIsReady => {
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
                _ => println!("Unimplemented: {:?}", op_code)
            },
            Err(_) => println!("Unknown op code: {}", raw_op_code)
        }

        Ok(result_packets)
    }
    
}

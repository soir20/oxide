use std::{collections::HashMap, fs::File, path::Path};

use crate::{
    game_server::{
        packets::{
            chat::{MessagePayload, MessageTypeData, SendMessage},
            housing::{BuildArea, HouseInfo, HouseInstanceData, InnerInstanceData, RoomInstances},
            tunnel::TunneledPacket,
            ui::{ExecuteScriptWithIntParams, ExecuteScriptWithStringParams},
            GamePacket, Name, Pos,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    ConfigError,
};

use serde::Deserialize;

use super::{
    character::{coerce_to_broadcast_supplier, CharacterType, Role},
    lock_enforcer::CharacterLockRequest,
    unique_guid::player_guid,
    zone::teleport_within_zone,
    WriteLockingBroadcastSupplier,
};

#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CommandEntry {
    pub description: String,
    pub usage: String,
    pub permission_level: Role,
    pub notes: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CommandConfig {
    pub commands: HashMap<String, CommandEntry>,
}

pub fn load_commands(config_dir: &Path) -> Result<CommandConfig, ConfigError> {
    let mut file = File::open(config_dir.join("commands.yaml"))?;
    let config: CommandConfig = serde_yaml::from_reader(&mut file)?;
    Ok(config)
}

fn server_msg(sender: u32, msg: &str) -> Vec<Broadcast> {
    vec![Broadcast::Single(
        sender,
        vec![
            // Print to chat
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SendMessage {
                    message_type_data: MessageTypeData::World,
                    payload: MessagePayload {
                        sender_guid: 0,
                        target_guid: 0,
                        channel_name: Name::default(),
                        target_name: Name::default(),
                        message: msg.into(),
                        pos: Pos::default(),
                        squad_guid: 0,
                        language_id: 0,
                    },
                },
            }),
            // Print to console
            GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: SendMessage {
                    message_type_data: MessageTypeData::System,
                    payload: MessagePayload {
                        sender_guid: 0,
                        target_guid: 0,
                        channel_name: Name::default(),
                        target_name: Name::default(),
                        message: msg.into(),
                        pos: Pos::default(),
                        squad_guid: 0,
                        language_id: 0,
                    },
                },
            }),
        ],
    )]
}

fn args_len_is_less_than(args: &[String], min_len: usize) -> bool {
    args.len() < min_len
}

pub fn command_details(sender: u32, entry: &CommandEntry) -> Vec<Broadcast> {
    let mut msg = format!(
        "Description: {}\nUsage: {}\n",
        entry.description, entry.usage
    );

    if let Some(notes) = &entry.notes {
        if !notes.is_empty() {
            msg.push_str("Notes:\n");
            for note in notes {
                msg.push_str(&format!("  - {}\n", note));
            }
        }
    }

    server_msg(sender, &msg)
}

fn command_error(sender: u32, error: &str, info: &CommandEntry) -> Vec<Broadcast> {
    let text = format!("Error: {}\nUsage: {}", error, info.usage);
    server_msg(sender, &text)
}

fn resolve_relative_coord(current_pos: f32, input: &str) -> Result<f32, String> {
    if let Some(offset) = input.strip_prefix('~') {
        if offset.is_empty() {
            Ok(current_pos)
        } else {
            offset
                .parse::<f32>()
                .map(|offset| current_pos + offset)
                .map_err(|_| input.to_string())
        }
    } else {
        input.parse::<f32>().map_err(|_| input.to_string())
    }
}

pub fn process_chat_command(
    sender: u32,
    arguments: &[String],
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let requester_guid = player_guid(sender);
    let commands_registry = game_server.commands.commands.clone();

    let broadcast_supplier: WriteLockingBroadcastSupplier = game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![requester_guid],
            character_consumer: move |_, _, mut characters_write, _| {
                let Some(requester_read_handle) = characters_write.get_mut(&requester_guid) else {
                    return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                };

                let player_stats = match &mut requester_read_handle.stats.character_type {
                    CharacterType::Player(player) => player.as_mut(),
                    _ => {
                        return coerce_to_broadcast_supplier(move |_| {
                            Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!(
                                    "Received chat command from {sender} but they were not a player"
                                ),
                            ))
                        });
                    }
                };

                let available_commands: Vec<(&String, &CommandEntry)> = commands_registry
                    .iter()
                    .filter(|(_, entry)| player_stats.role.has_permission(entry.permission_level))
                    .collect();

                let has_any_permission = !available_commands.is_empty();

                let response = {
                    let Some(cmd) = arguments.first().cloned() else {
                        if has_any_permission {
                            return coerce_to_broadcast_supplier(move |_| {
                                Ok(server_msg(sender, "Use ./help for a list of available commands."))
                            });
                        } else {
                            return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                        }
                    };

                    let Some(cmd_entry) = commands_registry.get(&cmd) else {
                        if has_any_permission {
                            let msg = format!(
                                "Command {cmd} was not found in the registry. Use ./help for a list of available commands."
                            );
                            return coerce_to_broadcast_supplier(move |_| Ok(server_msg(sender, &msg)));
                        } else {
                            return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                        }
                    };

                    if !player_stats.role.has_permission(cmd_entry.permission_level) {
                        return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                    }

                    if arguments.iter().any(|arg| arg == "-h" || arg == "--help") {
                        let out = command_details(sender, cmd_entry);
                        return coerce_to_broadcast_supplier(move |_| Ok(out));
                    }

                    let err = move |msg: &str| {
                        let cmd_err = command_error(sender, msg, cmd_entry);
                        coerce_to_broadcast_supplier(move |_| Ok(cmd_err))
                    };

                    match cmd.as_str() {
                        "help" => {
                            let mut msg = "Available commands:\n".to_string();
                            msg.push_str(
                                "Use ./<command> with the help flag (-h or --help) to list command-specific info\n\n",
                            );

                            for (i, (name, entry)) in available_commands.iter().enumerate() {
                                msg.push_str(&format!(
                                    "  ./{} - {}\n    Usage: {}\n",
                                    name, entry.description, entry.usage
                                ));

                                if let Some(notes) = &entry.notes {
                                    if !notes.is_empty() {
                                        msg.push_str("    Notes:\n");
                                        for note in notes {
                                            msg.push_str(&format!("      - {}\n", note));
                                        }
                                    }
                                }

                                if i + 1 < available_commands.len() {
                                    msg.push('\n');
                                }
                            }

                            server_msg(sender, &msg)
                        }

                        "console" => {
                            player_stats.toggles.console = !player_stats.toggles.console;

                            let script = if player_stats.toggles.console {
                                "Console.show"
                            } else {
                                "Console.hide"
                            };

                            vec![Broadcast::Single(
                                sender,
                                vec![GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: ExecuteScriptWithStringParams {
                                        script_name: script.to_string(),
                                        params: vec![],
                                    },
                                })],
                            )]
                        }

                        "script" => {
                            if args_len_is_less_than(arguments, 2) {
                                return err("No arguments were provided");
                            }

                            let script_name = &arguments[1];
                            let params: Vec<String> =
                                arguments.iter().skip(2).cloned().collect();

                            vec![Broadcast::Single(
                                sender,
                                vec![GamePacket::serialize(&TunneledPacket {
                                    unknown1: true,
                                    inner: ExecuteScriptWithStringParams {
                                        script_name: script_name.to_string(),
                                        params,
                                    },
                                })],
                            )]
                        }

                        "loc" => {
                            let pos = requester_read_handle.stats.pos;
                            let rot = requester_read_handle.stats.rot;

                            let msg = format!(
                                "Position: {}, {}, {}\nRotation: {} {} {}",
                                pos.x, pos.y, pos.z,
                                rot.x, rot.y, rot.z,
                            );

                            server_msg(sender, &msg)
                        }

                        "tp" => {
                            if args_len_is_less_than(arguments, 4) {
                                return err("Not enough arguments provided");
                            }

                            let current_pos = requester_read_handle.stats.pos;

                            let x = match resolve_relative_coord(current_pos.x, &arguments[1]) {
                                Ok(coord) => coord,
                                Err(input) => return err(&format!("Invalid X coordinate: {}", input)),
                            };

                            let y = match resolve_relative_coord(current_pos.y, &arguments[2]) {
                                Ok(coord) => coord,
                                Err(input) => return err(&format!("Invalid Y coordinate: {}", input)),
                            };

                            let z = match resolve_relative_coord(current_pos.z, &arguments[3]) {
                                Ok(coord) => coord,
                                Err(input) => return err(&format!("Invalid Z coordinate: {}", input)),
                            };

                            let destination_pos = Pos { x, y, z, w: current_pos.w };
                            let destination_rot = requester_read_handle.stats.rot;

                            teleport_within_zone(sender, destination_pos, destination_rot)
                        }

                        "clicktp" => {
                            player_stats.toggles.click_to_teleport =
                                !player_stats.toggles.click_to_teleport;
                            vec![Broadcast::Single(sender, vec![])]
                        }

                        "freecam" => {
                            player_stats.toggles.free_camera = !player_stats.toggles.free_camera;
                            make_freecam_packets(sender, requester_guid, player_stats.toggles.free_camera)
                        }

                        _ => {
                            server_msg(sender, &format!(
                                "Command {cmd} exists in the registry but has no handler."
                            ))
                        }
                    }
                };

                coerce_to_broadcast_supplier(move |_| Ok(response))
            },
        });

    broadcast_supplier?(game_server)
}

fn make_freecam_packets(sender: u32, requester_guid: u64, enabled: bool) -> Vec<Broadcast> {
    if enabled {
        vec![Broadcast::Single(
            sender,
            vec![
                // Enable freecam incase it's disabled so the user doesn't have to open house settings and toggle it manually
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: ExecuteScriptWithIntParams {
                        script_name: "GameOptions.SetFreeFlyHousingEdit".to_string(),
                        params: vec![1],
                    },
                }),
                // Necessary because build area defines the freecam boundary
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: HouseInstanceData {
                        inner: InnerInstanceData {
                            house_guid: 0,
                            owner_guid: requester_guid,
                            owner_name: "".to_string(),
                            unknown3: 0,
                            house_name: 0,
                            player_given_name: "".to_string(),
                            unknown4: 0,
                            max_fixtures: 0,
                            unknown6: 0,
                            placed_fixture: vec![],
                            unknown7: false,
                            unknown8: 0,
                            unknown9: 0,
                            unknown10: false,
                            unknown11: 0,
                            unknown12: false,
                            build_areas: vec![BuildArea {
                                min: Pos {
                                    x: f32::MIN,
                                    y: f32::MIN,
                                    z: f32::MIN,
                                    w: 1.0,
                                },
                                max: Pos {
                                    x: f32::MAX,
                                    y: f32::MAX,
                                    z: f32::MAX,
                                    w: 1.0,
                                },
                            }],
                            house_icon: 0,
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
                // Enable edit mode to enter Free Camera
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: HouseInfo {
                        edit_mode_enabled: true,
                        unknown2: 0,
                        unknown3: true,
                        fixtures: 0,
                        unknown5: 0,
                        unknown6: 0,
                        unknown7: 0,
                    },
                }),
            ],
        )]
    } else {
        vec![Broadcast::Single(
            sender,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: HouseInfo {
                    edit_mode_enabled: false,
                    unknown2: 0,
                    unknown3: false,
                    fixtures: 0,
                    unknown5: 0,
                    unknown6: 0,
                    unknown7: 0,
                },
            })],
        )]
    }
}

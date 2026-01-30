use crate::game_server::{
    packets::{
        chat::{MessagePayload, MessageTypeData, SendMessage},
        housing::{BuildArea, HouseInfo, HouseInstanceData, InnerInstanceData, RoomInstances},
        tunnel::TunneledPacket,
        ui::{ExecuteScriptWithIntParams, ExecuteScriptWithStringParams},
        GamePacket, Name, Pos,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::{
    character::{coerce_to_broadcast_supplier, CharacterType},
    lock_enforcer::CharacterLockRequest,
    unique_guid::player_guid,
    zone::teleport_within_zone,
    WriteLockingBroadcastSupplier,
};

struct CommandInfo {
    name: &'static str,
    description: &'static str,
    usage: &'static str,
}

static COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "help",
        description: "Show a list of available commands.",
        usage: "./help",
    },
    CommandInfo {
        name: "console",
        description: "Toggles the client console for debugging.",
        usage: "./console",
    },
    CommandInfo {
        name: "script",
        description: "Run a client script with optional parameters.",
        usage: "./script <script name> [params...]",
    },
    CommandInfo {
        name: "tp",
        description: "Teleport to a set of coordinates.",
        usage: "./tp <x> <y> <z>",
    },
    CommandInfo {
        name: "clicktp",
        description: "Toggles click to teleport instead of click to move.",
        usage: "./clicktp",
    },
    CommandInfo {
        name: "freecam",
        description: "Toggles free camera.",
        usage: "./freecam (Must be used again to properly exit; other methods can break clickable NPCs until doing so)",
    },
];

fn server_msg(sender: u32, msg: &str) -> Vec<Broadcast> {
    vec![Broadcast::Single(
        sender,
        vec![
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

fn args_len_is_less_than(args: &[String], n: usize) -> bool {
    args.len() < n
}

fn command_details(sender: u32, info: &CommandInfo) -> Vec<Broadcast> {
    let text = format!("{}\nUsage: {}", info.description, info.usage);
    server_msg(sender, &text)
}

fn command_error(sender: u32, error: &str, info: &CommandInfo) -> Vec<Broadcast> {
    let text = format!("Error: {}\nUsage: {}", error, info.usage);
    server_msg(sender, &text)
}

pub fn process_chat_command(
    sender: u32,
    arguments: &[String],
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let requester_guid = player_guid(sender);

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

                let response = {
                    let Some(cmd) = arguments.first().cloned() else {
                        return coerce_to_broadcast_supplier(move |_| {
                            Ok(server_msg(
                                sender,
                                "Use ./help for a list of available commands.",
                            ))
                        });
                    };

                    let (cmd, cmd_info) =
                        if let Some(info) = COMMANDS.iter().find(|c| c.name == cmd) {
                            (info.name, info)
                        } else {
                            let msg = format!("Unknown command {cmd}");
                            return coerce_to_broadcast_supplier(move |_| {
                                Ok(server_msg(sender, &msg))
                            });
                        };

                    if arguments
                        .get(1)
                        .map(|f| f == "-h" || f == "-help")
                        .unwrap_or(false)
                    {
                        return coerce_to_broadcast_supplier(move |_| {
                            Ok(command_details(sender, cmd_info))
                        });
                    }

                    let err = |msg: &str| {
                        let msg = msg.to_string();
                        coerce_to_broadcast_supplier(move |_| {
                            Ok(command_error(sender, &msg, cmd_info))
                        })
                    };

                    // TODO: Gate certain commands behind a moderator check
                    match cmd {
                        "help" => {
                            let mut msg = "Available commands:\n".to_string();
                            msg.push_str(
                                "Use ./<command> with the help flag (-h or -help) to list command-specific info\n\n",
                            );

                            for cmd in COMMANDS {
                                msg.push_str(&format!(
                                    "  ./{} - {}\n    Usage: {}\n\n",
                                    cmd.name, cmd.description, cmd.usage
                                ));
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

                        "tp" => {
                            if args_len_is_less_than(arguments, 4) {
                                return err("Not enough arguments provided");
                            }

                            let coords = (
                                arguments[1].parse::<f32>(),
                                arguments[2].parse::<f32>(),
                                arguments[3].parse::<f32>(),
                            );

                            let (x, y, z) = match coords {
                                (Ok(x), Ok(y), Ok(z)) => (x, y, z),
                                _ => return err("Invalid arguments provided"),
                            };

                            let destination_pos = Pos { x, y, z, w: 1.0 };
                            let destination_rot = requester_read_handle.stats.rot;

                            teleport_within_zone(sender, destination_pos, destination_rot)
                        }

                        "clicktp" => {
                            player_stats.toggles.click_to_teleport =
                                !player_stats.toggles.click_to_teleport;

                                vec![Broadcast::Single(sender, vec![])]
                        }

                        "freecam" => {
                            player_stats.toggles.free_camera =
                                !player_stats.toggles.free_camera;

                            if player_stats.toggles.free_camera {
                                vec![Broadcast::Single(
                                    sender,
                                    vec![
                                        GamePacket::serialize(&TunneledPacket {
                                            unknown1: true,
                                            inner: ExecuteScriptWithIntParams {
                                                script_name:
                                                    "GameOptions.SetFreeFlyHousingEdit".to_string(),
                                                params: vec![1],
                                            },
                                        }),
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
                                                            x: -100000.0,
                                                            y: -100000.0,
                                                            z: -100000.0,
                                                            w: 1.0,
                                                        },
                                                        max: Pos {
                                                            x: 100000.0,
                                                            y: 100000.0,
                                                            z: 100000.0,
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
                        _ => {
                            let msg = format!(
                                "Command {cmd} exists in the registry but has no handler."
                            );
                            server_msg(sender, &msg)
                        }
                    }
                };

                coerce_to_broadcast_supplier(move |_| Ok(response))
            },
        });

    broadcast_supplier?(game_server)
}

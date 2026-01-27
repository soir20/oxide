use crate::game_server::{
    packets::{
        chat::{MessagePayload, MessageTypeData, SendMessage},
        tunnel::TunneledPacket,
        ui::ExecuteScriptWithStringParams,
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
        name: "toggleconsole",
        description: "Toggles the client console for debugging.",
        usage: "./toggleconsole",
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

fn has_no_arguments(args: &[String]) -> bool {
    args.len() < 2
}

fn args_len_is_less_than(args: &[String], n: usize) -> bool {
    args.len() < n
}

fn command_details(sender: u32, name: &str) -> Vec<Broadcast> {
    let text = if let Some(cmd) = COMMANDS.iter().find(|c| c.name == name) {
        format!("{}\nUsage: {}", cmd.description, cmd.usage)
    } else {
        format!("No information found for {}", name)
    };

    server_msg(sender, &text)
}

fn command_error(sender: u32, name: &str, error: &str) -> Vec<Broadcast> {
    let usage_text = if let Some(cmd) = COMMANDS.iter().find(|c| c.name == name) {
        format!("Usage: {}", cmd.usage)
    } else {
        format!("No usage details found for {}", name)
    };

    let text = format!("Error: {}\n{}", error, usage_text);

    server_msg(sender, &text)
}

pub fn process_chat_command(
    sender: u32,
    arguments: &[String],
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let sender_guid = player_guid(sender);

    let broadcast_supplier: WriteLockingBroadcastSupplier = game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![sender_guid],
            character_consumer: move |_, _, mut characters_write, _| {
                let Some(sender_handle) = characters_write.get_mut(&sender_guid) else {
                    return coerce_to_broadcast_supplier(|_| Ok(Vec::new()));
                };

                let player_stats = match &mut sender_handle.stats.character_type {
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
                    // TODO: Gate certain commands behind a moderator check
                    let Some(command) = arguments.first().cloned() else {
                        return coerce_to_broadcast_supplier(move |_| {
                            Ok(server_msg(
                                sender,
                                "Use ./help for a list of available commands.",
                            ))
                        });
                    };

                    match command.as_str() {
                        "help" => {
                            let mut msg = String::from("Available commands:\n");
                            for cmd in COMMANDS {
                                msg.push_str(&format!(
                                    "  ./{} - {}\n    Usage: {}\n",
                                    cmd.name, cmd.description, cmd.usage
                                ));
                            }
                            server_msg(sender, &msg)
                        }

                        "toggleconsole" => {
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
                            if has_no_arguments(arguments) {
                                return coerce_to_broadcast_supplier(move |_| {
                                    Ok(command_details(sender, &command))
                                });
                            }

                            let script_name = &arguments[1];
                            let params: Vec<String> = arguments.iter().skip(2).cloned().collect();

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
                            let err = |msg: &str| {
                                let msg = msg.to_string();
                                coerce_to_broadcast_supplier(move |_| {
                                    Ok(command_error(sender, &command, &msg))
                                })
                            };

                            if args_len_is_less_than(arguments, 4) {
                                return err("Not enough arguments");
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
                            let destination_rot = sender_handle.stats.rot;

                            teleport_within_zone(sender, destination_pos, destination_rot)
                        }
                        _ => {
                            let msg = format!("Unknown command {command}");
                            server_msg(sender, &msg)
                        }
                    }
                };

                coerce_to_broadcast_supplier(move |_| Ok(response))
            },
        });

    broadcast_supplier?(game_server)
}

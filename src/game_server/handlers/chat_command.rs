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

use crate::game_server::handlers::character::Player;

use crate::game_server::handlers::inventory::wield_type_from_inventory;

use serde::Deserialize;

use super::{
    character::{coerce_to_broadcast_supplier, CharacterType, Role},
    lock_enforcer::CharacterLockRequest,
    unique_guid::player_guid,
    zone::teleport_within_zone,
    WriteLockingBroadcastSupplier,
};

pub const WEAPON_MOVES: &[(&str, u32, u8, usize)] = &[
    ("weaponmove1", 2237, 1, 0),
    ("weaponmove2", 2238, 1, 1),
    ("weaponmove3", 2239, 1, 2),
    ("weaponmovetaunt", 2797, 2, 0),
    ("weaponmovethrow", 2798, 2, 1),
];

pub const FORCE_PUSH_HAND_OFFSET: (f32, f32, f32) = (1.5, 1.75, -0.2);

pub const ATTACHED: (f32, f32, f32) = (f32::NAN, f32::NAN, f32::NAN);

pub fn effect_pos(pos: Pos, rot: Pos, (forward, up, right): (f32, f32, f32)) -> Pos {
    if forward.is_nan() {
        return Pos::default();
    }
    Pos {
        x: pos.x + rot.x * forward + rot.z * right,
        y: pos.y + up,
        z: pos.z + rot.z * forward - rot.x * right,
        w: pos.w,
    }
}

/// (forward, up, right) offset from the player, or `ATTACHED`.
pub type EffectOffset = (f32, f32, f32);

/// (item_guid, composite_effect_id, effect_offset, delay_millis)
pub type EmoteEffect = (u32, u32, EffectOffset, u32);

/// (name, item_guid, animation_id, effect_offset, [(composite_effect_id, delay_millis)])
pub type MindTrick = (
    &'static str,
    u32,
    Option<i32>,
    EffectOffset,
    &'static [(u32, u32)],
);

pub const EMOTE_EFFECTS: &[EmoteEffect] = &[
    // Force Push emote -> PFX_ForcePush_Emote, same as the mind trick
    (6021, 2085, ATTACHED, 0),
    // Jedi: Force Pound -> PFX_CMBT_ForceExplosion_GroundRipple, in front where the
    // fist hits the ground (same spot/timing as the Force Explosion mind trick)
    (6028, 2875, (2.0, 0.0, -0.3), 500),
    // Sith: Force Lightning -> PFX_ForceLightning_Arc_Loop (next to the Rage /
    // Seethe emote effects): orb 0.6-3.5 s, then lightning arcs until 9.5 s.
    // ATTACHED: played on the character (bone-attached) rather than at a point.
    (6036, 2872, ATTACHED, 0),
    // Sith: Rage / Sith: Seethe -> their own force bursts, at chest height
    (6031, 2874, ATTACHED, 0),
    (6032, 2876, ATTACHED, 0),
    // Force Lift -> PFX_Force_Glow_hands (glow on both hands)
    (6020, 2114, ATTACHED, 0),
    // Force Meditate (moves.yaml) and Jedi: Meditate -> PFX_Meditation_Force_Distortion
    (3096, 2766, ATTACHED, 0),
    (6030, 2766, ATTACHED, 0),
];

pub const HELD_EMOTES: &[(u32, i32, f32)] = &[];

pub const EMOTE_DURATIONS: &[(i32, f32)] = &[
    (3608, 9.5), // Emo_Sith_ForceLightning (PFX_ForceLightning_Arc_Loop)
    (3401, 6.0), // Emo_PortraitPose1 (Pose)
    (3301, 4.0), // Emo_Standard_Flourish1 (Battle Class Flourish I)
];

pub fn emote_duration_seconds(animation_id: i32) -> f32 {
    EMOTE_DURATIONS
        .iter()
        .find(|(id, _)| *id == animation_id)
        .map(|(_, s)| *s)
        .unwrap_or(2.0)
}

pub const MIND_TRICKS: &[MindTrick] = &[
    ("fireworks", 565, None, (0.0, 0.0, 0.0), &[(1722, 0)]),
    (
        "forceexplosion",
        662,
        Some(3605),
        (2.0, 0.0, -0.3),
        &[(1752, 500)],
    ),
    ("forcepushtrick", 2084, Some(3015), ATTACHED, &[(2085, 0)]),
];

pub fn mind_trick_effect_duration_millis(composite_effect_id: u32) -> u32 {
    match composite_effect_id {
        1722 => 10_000,       // PFX_Fireworks_OverHead_sm (defaultLifeTime 10 s)
        2085 => 2_000,        // PFX_ForcePush_Emote (second burst triggers at 0.7 s)
        2695 => 4_000,        // PFX_Sith_KML_ForceLightning
        2872 => 10_000,       // PFX_ForceLightning_Arc_Loop (arcs end at 9.5 s)
        2766 => 4_000,        // PFX_Meditation_Force_Distortion
        2874..=2876 => 2_000, // Rage / Force Pound / Seethe bursts
        2114 => 4_000,        // PFX_Force_Glow_hands (Force Lift)
        _ => 1000,
    }
}

pub const PERSISTENT_MIND_TRICKS: &[(&str, u32, u32, u32)] = &[
    ("protontorpedoes", 566, 1715, 9101),
    ("fighterbattle", 664, 1789, 9102),
    ("republiccruiser", 567, 1716, 9103),
    ("malevolence", 630, 1730, 9104),
    ("forceglow", 663, 1793, 9105),
    ("orbitingstarfighters", 250020, 2109, 9106),
    ("orbitingvultures", 250021, 2111, 9107),
    ("orbitingywings", 250022, 2110, 9108),
    ("forceglowhands", 250023, 2114, 9109),
];

pub const HOLOPROJECTOR_MODELS: &[(u32, u32)] = &[
    // (item_guid, model_id)
    (443, 1041),  // Super Battle Droid
    (744, 1038),  // Battle Droid
    (745, 1039),  // Commando Droid
    (746, 1040),  // Droideka
    (747, 1042),  // Bith
    (1418, 1559), // Jawa
    (2106, 1109), // Gotal
    (2107, 1110), // Ithorian
    (2108, 1111), // Gungan
    (2234, 1283), // Geonosian
    (2235, 1284), // Brain Worm
    (2386, 1409), // Mortis Daughter
    (2387, 1410), // Mortis Son
    (2450, 1713), // Rancor
    (2753, 1493), // Chewbacca
    (2754, 1494), // Underworld Ithorian
    (2772, 1555), // Yoda
    (2807, 1613), // General Grievous
    (2808, 1614), // Talz
    (2831, 1626), // Jabba
    (2832, 1627), // Cad Bane
    (2890, 1709), // Gamorrean Guard
    (2891, 1710), // Seripas (variant 2)
    (2898, 1714), // Savage Opress
    (2906, 1820), // Ziro
    (2907, 1821), // Kowakian Monkey-Lizard
    (2913, 1871), // Captain Ackbar
    (2914, 1872), // Nossor Ri
    (2915, 1873), // Karkarodon
    (2920, 1931), // Orphne
    (3070, 1578), // King Manchucho
    (3071, 1939), // Magnaguard
    (3222, 2165), // Sniper Droid
    (3405, 2334), // Gundark
    (3433, 2340), // C-21 Highsinger
    (3434, 2339), // Darth Maul
    (3439, 2341), // Plo Koon
    (3441, 2352), // Kage Warrior
    (3442, 2356), // Ortolan
    (3443, 1965), // Umbaran Soldier
    (3444, 2470), // Narglatch
    (3445, 2436), // Old Daka
];

pub fn weapon_move_animation_id(
    player_stats: &Player,
    game_server: &GameServer,
    pack: u8,
    move_index: usize,
) -> Option<i32> {
    let equipped_items = player_stats
        .inventory
        .equipped_items(player_stats.inventory.active_battle_class);
    let wield_type = wield_type_from_inventory(&equipped_items, game_server);
    let (pack1, pack2) = wield_type.flourish_packs()?;
    Some(if pack == 1 {
        pack1[move_index]
    } else {
        pack2[move_index]
    })
}
#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CommandEntry {
    pub description: String,
    pub usage: String,
    pub permission_level: Role,
    #[serde(default)]
    pub notes: Vec<String>,
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

fn args_len_is_less_than(args: &[String], min_len: usize) -> bool {
    args.len() < min_len
}

pub fn command_details(sender: u32, entry: &CommandEntry) -> Vec<Broadcast> {
    let mut msg = format!(
        "Description: {}\nUsage: {}\n",
        entry.description, entry.usage
    );

    if !entry.notes.is_empty() {
        msg.push_str("Notes:\n");
        for note in entry.notes.iter() {
            msg.push_str(&format!("  - {}\n", note));
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

                                if !entry.notes.is_empty() {
                                    msg.push_str("    Notes:\n");
                                    for note in entry.notes.iter() {
                                        msg.push_str(&format!("      - {}\n", note));
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
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: ExecuteScriptWithIntParams {
                        script_name: "GameOptions.SetFreeFlyHousingEdit".to_string(),
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

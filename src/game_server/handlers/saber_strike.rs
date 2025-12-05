use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::character::{MinigameStatus, Player},
    packets::{
        minigame::{MinigameHeader, ScoreEntry, ScoreType},
        player_update::UpdateOwner,
        saber_strike::{
            SaberStrikeGameOver, SaberStrikeObfuscatedScore, SaberStrikeOpCode,
            SaberStrikeSingleKill, SaberStrikeThrowKill,
        },
        tower_defense::{
            TowerDefenseAerialPath, TowerDefenseDeck, TowerDefenseEnemyGroup,
            TowerDefenseEnemyType, TowerDefenseInventoryItem, TowerDefenseNotify,
            TowerDefenseOpCode, TowerDefenseSpecialDefinition, TowerDefenseStageData,
            TowerDefenseStartGame, TowerDefenseState, TowerDefenseTowerDefinition,
            TowerDefenseWaves, TowerTransaction, TowerTransactionType,
        },
        tunnel::TunneledPacket,
        ui::ExecuteScriptWithStringParams,
        CharacterBoneNameTarget, GamePacket, GuidTarget, Pos, Target,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::minigame::{handle_minigame_packet_write, MinigameTypeData};

pub fn start_saber_strike(
    saber_strike_stage_id: u32,
    player: &Player,
    minigame_status: &MinigameStatus,
    game_server: &GameServer,
) -> Vec<Vec<u8>> {
    vec![
        /*GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: ExecuteScriptWithStringParams {
                script_name: "TowerDefenseHandler.show".to_string(),
                params: vec![],
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: ExecuteScriptWithStringParams {
                script_name: "TowerDefenseHandler.onStart".to_string(),
                params: vec![],
            },
        }),*/
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerDefenseStageData {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::StageData as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::StageData as i32,
                unknown_header_boolean: false,
                tower_definitions: vec![
                    TowerDefenseTowerDefinition {
                        guid: 1,
                        guid2: 1,
                        rank: 1,
                        name_id: 2,
                        tower_type: 10,
                        energy_cost: 4,
                        sell_value: 50,
                        damage: 0.6,
                        range: 0.7,
                        upgraded_tower_guid: 2,
                        icon_id: 90,
                        firing_rate: 0.1,
                        can_attack_aerial: true,
                        can_attack_ground: true,
                        unknown14: false,
                        required: false,
                        unknown16: false,
                        description_id: 11,
                        shield_damage: 120,
                    },
                    TowerDefenseTowerDefinition {
                        guid: 2,
                        guid2: 2,
                        rank: 2,
                        name_id: 10,
                        tower_type: 10,
                        energy_cost: 10,
                        sell_value: 10,
                        damage: 0.5,
                        range: 0.75,
                        upgraded_tower_guid: 10,
                        icon_id: 10,
                        firing_rate: 1.0,
                        can_attack_aerial: false,
                        can_attack_ground: false,
                        unknown14: false,
                        required: false,
                        unknown16: false,
                        description_id: 10,
                        shield_damage: 10,
                    },
                    TowerDefenseTowerDefinition {
                        guid: 3,
                        guid2: 3,
                        rank: 1,
                        name_id: 10,
                        tower_type: 10,
                        energy_cost: 0,
                        sell_value: 0,
                        damage: 0.0,
                        range: 0.0,
                        upgraded_tower_guid: 0,
                        icon_id: 0,
                        firing_rate: 0.0,
                        can_attack_aerial: false,
                        can_attack_ground: false,
                        unknown14: false,
                        required: false,
                        unknown16: false,
                        description_id: 0,
                        shield_damage: 0,
                    },
                    TowerDefenseTowerDefinition {
                        guid: 4,
                        guid2: 4,
                        rank: 1,
                        name_id: 0,
                        tower_type: 0,
                        energy_cost: 0,
                        sell_value: 0,
                        damage: 0.0,
                        range: 0.0,
                        upgraded_tower_guid: 0,
                        icon_id: 0,
                        firing_rate: 0.0,
                        can_attack_aerial: false,
                        can_attack_ground: false,
                        unknown14: false,
                        required: false,
                        unknown16: false,
                        description_id: 0,
                        shield_damage: 0,
                    },
                ],
                special_definitions: vec![
                    TowerDefenseSpecialDefinition {
                        guid: 10,
                        guid2: 10,
                        name_id: 2,
                        damage: 0.5,
                        icon_id: 3,
                        description_id: 4,
                        unknown6: false,
                    },
                    TowerDefenseSpecialDefinition {
                        guid: 11,
                        guid2: 11,
                        name_id: 2,
                        damage: 0.5,
                        icon_id: 3,
                        description_id: 4,
                        unknown6: false,
                    },
                ],
                fixed_camera_pos: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                fixed_look_at: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 1.0,
                },
                fixed_field_of_view: 100.0,
                pan_origin: Pos {
                    x: 300.0,
                    y: 0.0,
                    z: 225.0,
                    w: 1.0,
                },
                pan_max_scale: Pos {
                    x: 1.5,
                    y: 0.0,
                    z: 0.0,
                    w: 1.0,
                },
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerDefenseDeck {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::Deck as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::Deck as i32,
                unknown_header_boolean: false,
                towers: vec![TowerDefenseInventoryItem {
                    guid: 1,
                    required: false,
                }],
                specials: vec![],
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerDefenseWaves {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::Waves as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::Waves as i32,
                unknown_header_boolean: false,
                enemy_groups: vec![TowerDefenseEnemyGroup {
                    guid: 25,
                    guid2: 25,
                    wave_id: 25,
                    spawn_number: 2,
                    spawn_delay: 3,
                    icon_id: 4,
                    unknown6: 5,
                }],
                enemy_types: vec![TowerDefenseEnemyType {
                    guid: 30,
                    guid2: 30,
                    count: 1,
                    battle_class_icon_id: 2,
                    battle_class_background_icon_id: 3,
                    battle_class_rank: 4,
                    icon_id: 1000,
                    name_id: 6,
                    description_id: 7,
                    max_health: 8,
                    max_force_power: 9,
                    max_shield: 10,
                }],
                aerial_paths: vec![TowerDefenseAerialPath {
                    rail_id: 1,
                    rail_id2: 1,
                    unknown2: 1,
                    unknown3: 5,
                    unknown4: 5,
                }],
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerDefenseState {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::State as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::State as i32,
                unknown_header_boolean: false,
                energy: 100,
                score: 200,
                current_wave: 300,
                unknown4: 400,
                max_waves: 500,
                lives: 600,
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerDefenseNotify {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::Notify as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::Notify as i32,
                unknown_header_boolean: false,
                unknown1: 4,
                target: Target::Guid(GuidTarget {
                    fallback_pos: Pos::default(),
                    guid: 1,
                }),
                unknown2: 12,
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerDefenseStartGame {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::StartGame as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::StartGame as i32,
                unknown_header_boolean: false,
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerDefenseStartGame {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::StartGame as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::StartGame as i32,
                unknown_header_boolean: false,
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: TowerTransaction {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::TowerTransaction as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::TowerTransaction as i32,
                unknown_header_boolean: false,
                transaction_type: TowerTransactionType::Upgrade,
                new_tower_npc_guid: 1152923703630102728,
                base_guid: 1152921504606847176,
                old_tower_npc_guid: 1152922604118474952,
                new_base_texture_alias: "Rank2".to_string(),
                tower_definition_guid: 1,
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdateOwner {
                child_guid: 1152922604118474952,
                owner: Target::CharacterBone(CharacterBoneNameTarget {
                    fallback_pos: Pos::default(),
                    character_guid: 1152921504606847176,
                    bone_name: "BASE".to_string(),
                }),
                attach: true,
            },
        }),
        GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: UpdateOwner {
                child_guid: 1152923703630102728,
                owner: Target::CharacterBone(CharacterBoneNameTarget {
                    fallback_pos: Pos::default(),
                    character_guid: 1152921504606847176,
                    bone_name: "BASE".to_string(),
                }),
                attach: true,
            },
        }),
    ]
}

pub fn process_saber_strike_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let header = MinigameHeader::deserialize(cursor)?;
    match SaberStrikeOpCode::try_from(header.sub_op_code) {
        Ok(op_code) => match op_code {
            SaberStrikeOpCode::GameOver => {
                let game_over = SaberStrikeGameOver::deserialize(cursor)?;
                handle_saber_strike_game_over(&header, &game_over, sender, game_server)
            }
            SaberStrikeOpCode::SingleKill => {
                let _ = SaberStrikeSingleKill::deserialize(cursor)?;
                // TODO: update player achievement progress
                Ok(Vec::new())
            }
            SaberStrikeOpCode::ThrowKill => {
                let _ = SaberStrikeThrowKill::deserialize(cursor)?;
                // TODO: update player achievement progress
                Ok(Vec::new())
            }
            SaberStrikeOpCode::ObfuscatedScore => {
                let obfuscated_score_packet = SaberStrikeObfuscatedScore::deserialize(cursor)?;
                handle_minigame_packet_write(
                    sender,
                    game_server,
                    &header,
                    |minigame_status, _, _, _, _, _| {
                        match &mut minigame_status.type_data {
                            MinigameTypeData::SaberStrike { obfuscated_score } => {
                                *obfuscated_score = obfuscated_score_packet.score();
                                Ok(Vec::new())
                            },
                            _ => Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!("Player {sender} sent a Saber Strike obfuscated score packet, but they have no Saber Strike game data")
                            ))
                        }
                    },
                )
            }
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                Err(ProcessPacketError::new(
                    ProcessPacketErrorType::UnknownOpCode,
                    format!("Unimplemented minigame op code: {op_code:?} {buffer:x?}"),
                ))
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            Err(ProcessPacketError::new(
                ProcessPacketErrorType::UnknownOpCode,
                format!(
                    "Unknown minigame packet: {}, {buffer:x?}",
                    header.sub_op_code
                ),
            ))
        }
    }
}

fn handle_saber_strike_game_over(
    header: &MinigameHeader,
    game_over: &SaberStrikeGameOver,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    handle_minigame_packet_write(
        sender,
        game_server,
        header,
        |minigame_status, _, _, _, _, _| {
            let MinigameTypeData::SaberStrike { obfuscated_score } = minigame_status.type_data
            else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Player {sender} sent a Saber Strike game over packet, but they have no Saber Strike game data")
                ));
            };

            if obfuscated_score != game_over.total_score {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} sent a Saber Strike game over packet with score {}, but their obfuscated score was {obfuscated_score}",
                        game_over.total_score,
                    )
                ));
            }

            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_TotalTime".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Time,
                score_count: game_over.duration_seconds.round() as i32,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_ThrowsRemaining".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: game_over.remaining_sabers,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_TotalDestroyed".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: game_over.enemies_killed,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_BestThrow".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Counter,
                score_count: game_over.best_throw,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_TotalScore".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Total,
                score_count: game_over.total_score,
                score_max: 0,
                score_points: 0,
            });
            minigame_status.total_score = game_over.total_score;
            minigame_status.win_status.set_won(game_over.won);
            Ok(vec![Broadcast::Single(
                sender,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: ExecuteScriptWithStringParams {
                        script_name: "Ui.QuitMiniGame".to_string(),
                        params: Vec::new(),
                    },
                })],
            )])
        },
    )
}

use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::{
        character::{MinigameStatus, Player},
        inventory::player_has_saber_equipped,
    },
    packets::{
        minigame::{MinigameHeader, ScoreEntry, ScoreType},
        saber_strike::{
            SaberStrikeGameOver, SaberStrikeObfuscatedScore, SaberStrikeOpCode,
            SaberStrikeSingleKill, SaberStrikeStageData, SaberStrikeThrowKill,
        },
        tower_defense::{
            TowerDefenseDeck, TowerDefenseNotify, TowerDefenseOpCode, TowerDefenseStageData,
            TowerDefenseState, TowerDefenseUnknown, TowerDefenseWaves, UnknownDeckArray,
            UnknownRDArray1, UnknownRDArray2, UnknownWaveArray1, UnknownWaveArray2,
            UnknownWaveArray3,
        },
        tunnel::TunneledPacket,
        ui::ExecuteScriptWithStringParams,
        GamePacket, GuidTarget, Pos, Target,
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
                sub_op_code: TowerDefenseOpCode::StageData as u32,
                unknown_header_boolean: false,
                unknown_array1: vec![
                    UnknownRDArray1 {
                        guid: 1,
                        guid2: 1,
                        unknown2: 1,
                        unknown3: 1000,
                        unknown4: 1000,
                        unknown5: 1000,
                        unknown6: 1000,
                        unknown7: 3.0,
                        unknown8: 3.0,
                        unknown9: 1000,
                        unknown10: 1000,
                        unknown11: 10.0,
                        unknown12: true,
                        unknown13: true,
                        unknown14: true,
                        unknown15: true,
                        unknown16: true,
                        unknown17: 1000,
                        unknown18: 1000,
                    },
                    UnknownRDArray1 {
                        guid: 2,
                        guid2: 2,
                        unknown2: 2,
                        unknown3: 10,
                        unknown4: 10,
                        unknown5: 10,
                        unknown6: 10,
                        unknown7: 10.0,
                        unknown8: 10.0,
                        unknown9: 10,
                        unknown10: 10,
                        unknown11: 10.0,
                        unknown12: false,
                        unknown13: false,
                        unknown14: false,
                        unknown15: true,
                        unknown16: true,
                        unknown17: 10,
                        unknown18: 10,
                    },
                    UnknownRDArray1 {
                        guid: 3,
                        guid2: 3,
                        unknown2: 3,
                        unknown3: 10,
                        unknown4: 10,
                        unknown5: 0,
                        unknown6: 0,
                        unknown7: 0.0,
                        unknown8: 0.0,
                        unknown9: 0,
                        unknown10: 0,
                        unknown11: 0.0,
                        unknown12: true,
                        unknown13: true,
                        unknown14: false,
                        unknown15: false,
                        unknown16: false,
                        unknown17: 0,
                        unknown18: 0,
                    },
                    UnknownRDArray1 {
                        guid: 4,
                        guid2: 4,
                        unknown2: 4,
                        unknown3: 0,
                        unknown4: 0,
                        unknown5: 0,
                        unknown6: 0,
                        unknown7: 0.0,
                        unknown8: 0.0,
                        unknown9: 0,
                        unknown10: 0,
                        unknown11: 0.0,
                        unknown12: true,
                        unknown13: true,
                        unknown14: true,
                        unknown15: true,
                        unknown16: true,
                        unknown17: 0,
                        unknown18: 0,
                    },
                ],
                unknown_array2: vec![UnknownRDArray2 {
                    guid: 10,
                    guid2: 10,
                    unknown2: 10,
                    unknown3: 10.0,
                    unknown4: 10,
                    unknown5: 10,
                    unknown6: true,
                }],
                camera_pos: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                look_at: Pos {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
                field_of_view: 100.0,
                unknown_pos3: Pos {
                    x: 0.1,
                    y: 0.2,
                    z: 0.3,
                    w: 1.0,
                },
                unknown_pos4: Pos {
                    x: 0.4,
                    y: 0.5,
                    z: 0.6,
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
                sub_op_code: TowerDefenseOpCode::Deck as u32,
                unknown_header_boolean: false,
                unknown1: vec![UnknownDeckArray {
                    unknown1: 1000,
                    unknown2: false,
                }],
                unknown2: vec![UnknownDeckArray {
                    unknown1: 1000,
                    unknown2: true,
                }],
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
                sub_op_code: TowerDefenseOpCode::Waves as u32,
                unknown_header_boolean: false,
                unknown_array1: vec![UnknownWaveArray1 {
                    unknown1: 25,
                    unknown2: 25,
                    unknown3: 25,
                    unknown4: 25,
                    unknown5: 25,
                    unknown6: 25,
                }],
                unknown_array2: vec![UnknownWaveArray2 {
                    unknown1: 25,
                    unknown2: 25,
                    unknown3: 25,
                    unknown4: 25,
                    unknown5: 25,
                    unknown6: 25,
                    unknown7: 25,
                    unknown8: 25,
                    unknown9: 25,
                    unknown10: 25,
                    unknown11: 25,
                }],
                unknown_array3: vec![UnknownWaveArray3 {
                    unknown1: 25,
                    unknown2: 25,
                    unknown3: 25,
                    unknown4: 25,
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
                sub_op_code: TowerDefenseOpCode::State as u32,
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
                sub_op_code: TowerDefenseOpCode::Notify as u32,
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
            inner: TowerDefenseUnknown {
                minigame_header: MinigameHeader {
                    stage_guid: minigame_status.group.stage_guid,
                    sub_op_code: TowerDefenseOpCode::Unknown as i32,
                    stage_group_guid: minigame_status.group.stage_group_guid,
                },
                sub_op_code: TowerDefenseOpCode::Unknown as u32,
                unknown_header_boolean: false,
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

use std::io::{Cursor, Read};

use packet_serialize::DeserializePacket;

use crate::{
    game_server::{
        packets::{
            minigame::{MinigameHeader, ScoreEntry, ScoreType},
            saber_strike::{
                SaberStrikeGameOver, SaberStrikeObfuscatedScore, SaberStrikeOpCode,
                SaberStrikeSingleKill, SaberStrikeThrowKill,
            },
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    info,
};

use super::minigame::{end_active_minigame, handle_minigame_packet_write, MinigameTypeData};

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
                    |minigame_status, _, _, _| {
                        match &mut minigame_status.type_data {
                            MinigameTypeData::SaberStrike { obfuscated_score } => {
                                *obfuscated_score = obfuscated_score_packet.score();
                                Ok(Vec::new())
                            },
                            _ => Err(ProcessPacketError::new(
                                ProcessPacketErrorType::ConstraintViolated,
                                format!("Player {} sent a Saber Strike obfuscated score packet, but they have no Saber Strike game data", sender)
                            ))
                        }
                    },
                )
            }
            _ => {
                let mut buffer = Vec::new();
                cursor.read_to_end(&mut buffer)?;
                info!(
                    "Unimplemented minigame op code: {:?} {:x?}",
                    op_code, buffer
                );
                Ok(Vec::new())
            }
        },
        Err(_) => {
            let mut buffer = Vec::new();
            cursor.read_to_end(&mut buffer)?;
            info!(
                "Unknown minigame packet: {}, {:x?}",
                header.sub_op_code, buffer
            );
            Ok(Vec::new())
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
        |minigame_status, minigame_stats, _, stage_config| {
            if let MinigameTypeData::SaberStrike { obfuscated_score } = minigame_status.type_data {
                if obfuscated_score != game_over.total_score {
                    return Err(ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "Player {} sent a Saber Strike game over packet with score {}, but their obfuscated score was {}",
                            sender,
                            game_over.total_score,
                            obfuscated_score,
                        )
                    ));
                }
            } else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Player {} sent a Saber Strike game over packet, but they have no Saber Strike game data", sender)
                ));
            }

            minigame_status.score_entries.push(ScoreEntry {
                entry_text: "lt_TotalTime".to_string(),
                icon_set_id: 0,
                score_type: ScoreType::Time,
                score_count: i32::try_from(game_over.duration_seconds.round() as i64)
                    .unwrap_or_default(),
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
            minigame_status.game_won = game_over.won;
            if game_over.won {
                minigame_stats.complete(stage_config.stage_config.guid, game_over.total_score);
            }
            Ok(())
        },
    )?;

    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, zones_lock_enforcer| {
            zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                end_active_minigame(
                    sender,
                    characters_table_write_handle,
                    zones_table_write_handle,
                    header.stage_guid,
                    false,
                    game_server,
                )
            })
        },
    )
}

use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::{
    game_server::{handlers::guid::IndexedGuid, Broadcast, GameServer, TickableNpcSynchronization},
    info,
};

use super::{
    character::{Character, CharacterCategory, Chunk, MinigameMatchmakingGroup},
    guid::GuidTableIndexer,
    lock_enforcer::{CharacterLockRequest, MinigameDataLockEnforcer, MinigameDataLockRequest},
    minigame::{
        leave_active_minigame_if_any, prepare_active_minigame_instance, LeaveMinigameTarget,
        MatchmakingGroupStatus, SharedMinigameDataTickableIndex,
    },
    unique_guid::shorten_player_guid,
    zone::ZoneInstance,
};

const fn tickable_categories(
    synchronization: TickableNpcSynchronization,
) -> [CharacterCategory; 2] {
    [
        CharacterCategory::NpcTickable(synchronization),
        CharacterCategory::NpcAutoInteractableTickable(synchronization),
    ]
}

pub fn enqueue_tickable_chunks(
    game_server: &GameServer,
    synchronization: TickableNpcSynchronization,
    chunks_enqueue: Sender<(u64, Chunk, TickableNpcSynchronization)>,
) -> usize {
    game_server
        .lock_enforcer()
        .read_characters(|characters_table_read_handle| {
            let count = tickable_categories(synchronization)
                .into_iter()
                .flat_map(|category| {
                    let range = (category, u64::MIN, Character::MIN_CHUNK)
                        ..=(category, u64::MAX, Character::MAX_CHUNK);
                    characters_table_read_handle.indices1_by_range(range)
                })
                .fold(0, |count, (_, instance_guid, chunk)| {
                    chunks_enqueue
                        .send((instance_guid, chunk, synchronization))
                        .expect("Tickable channel disconnected");
                    count + 1
                });

            CharacterLockRequest {
                read_guids: Vec::new(),
                write_guids: Vec::new(),
                character_consumer: move |_, _, _, _| count,
            }
        })
}

pub fn tick_single_chunk(
    game_server: &GameServer,
    now: Instant,
    instance_guid: u64,
    chunk: Chunk,
    synchronization: TickableNpcSynchronization,
) -> Vec<Broadcast> {
    game_server.lock_enforcer().read_characters(|characters_table_read_handle| {
        let tickable_characters: Vec<u64> = tickable_categories(synchronization)
            .into_iter()
            .flat_map(|category| characters_table_read_handle.keys_by_index1((category, instance_guid, chunk)))
            .collect();

        let nearby_player_guids = ZoneInstance::all_players_nearby(chunk, instance_guid, characters_table_read_handle);
        let mut read_guids: Vec<u64> = nearby_player_guids.iter()
            .map(|guid| *guid as u64)
            .collect();

        for ticked_character_guid in tickable_characters.iter() {
            if let Some(synchronzied_character_guid) = characters_table_read_handle.index5(*ticked_character_guid) {
                read_guids.push(*synchronzied_character_guid);
            }
        }

        CharacterLockRequest {
            read_guids,
            write_guids: tickable_characters.clone(),
            character_consumer: move |_,
            characters_read,
            mut characters_write,
            _| {
                let mut broadcasts = Vec::new();

                for guid in tickable_characters.iter() {
                    let tickable_character = characters_write.get_mut(guid).unwrap();

                    if let Some(synchronize_with) = &tickable_character.synchronize_with {
                        if let Some(synchronized_character) =
                            characters_read.get(synchronize_with)
                        {
                            if let Some(synchronized_guid) = synchronized_character.synchronize_with {
                                panic!(
                                    "Cannot synchronize character {} to a character {} because they are synchronized to character {}",
                                    guid,
                                    synchronized_character.guid(),
                                    synchronized_guid
                                );
                            }

                            if synchronized_character.last_procedure_change() > tickable_character.last_procedure_change() {
                                if let Some(key) =
                                    synchronized_character.current_tickable_procedure()
                                {
                                    tickable_character.set_tickable_procedure_if_exists(key.clone(), now)
                                }
                            }
                        }
                    }

                    broadcasts.append(
                        &mut tickable_character.tick(now, &nearby_player_guids, &characters_read, game_server.mounts(), game_server.items(), game_server.customizations()),
                    );
                }

                broadcasts
            },
        }
    })
}

pub fn tick_matchmaking_groups(game_server: &GameServer) -> Vec<Broadcast> {
    let now = Instant::now();
    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, minigame_data_lock_enforcer| {
            minigame_data_lock_enforcer.write_minigame_data(|minigame_data_table_write_handle, zones_lock_enforcer| {
                let mut broadcasts = Vec::new();

                // Iterate over timed-out groups for every stage, since the number of stages remains
                // a fairly small constant, while there can theoretically be billions of matchmaking groups.
                for stage in game_server.minigames().stage_configs() {
                    let timeout =
                        Duration::from_millis(stage.stage_config.matchmaking_timeout_millis() as u64);
                    // Make sure max time is greater than or equal to start time so that the range is valid
                    let max_time = match now.checked_sub(timeout) {
                        Some(max_time) => max_time,
                        None => continue,
                    }.max(game_server.start_time);
                    let stage_group_guid = stage.stage_group_guid;
                    let stage_guid = stage.stage_config.guid();
                    let min_players = stage.stage_config.min_players();

                    let timed_out_group_range = (MatchmakingGroupStatus::Open, stage_guid, game_server.start_time)..=(MatchmakingGroupStatus::Open, stage_guid, max_time);
                    let timed_out_groups: Vec<MinigameMatchmakingGroup> = minigame_data_table_write_handle
                        .keys_by_index2_range(timed_out_group_range)
                        .collect();
                    for matchmaking_group in timed_out_groups {
                        let players_in_group: Vec<u32> = characters_table_write_handle
                            .keys_by_index4(&matchmaking_group)
                            .filter_map(|guid| shorten_player_guid(guid).ok())
                            .collect();

                        zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                            if players_in_group.len() as u32 >= min_players {
                                broadcasts.append(&mut prepare_active_minigame_instance(
                                    matchmaking_group,
                                    &players_in_group,
                                    &stage,
                                    characters_table_write_handle,
                                    minigame_data_table_write_handle,
                                    zones_table_write_handle,
                                    None,
                                    game_server,
                                ));
                                return;
                            }

                            if players_in_group.len() == 1 {
                                if let Some(replacement_stage_locator) = &stage.stage_config.single_player_stage() {
                                    if let Some(replacement_stage) = game_server
                                        .minigames()
                                        .stage_config(replacement_stage_locator.stage_group_guid, replacement_stage_locator.stage_guid)
                                    {
                                        if replacement_stage.stage_config.min_players() == 1 {
                                            broadcasts.append(&mut prepare_active_minigame_instance(
                                                matchmaking_group,
                                                &players_in_group,
                                                &replacement_stage,
                                                characters_table_write_handle,
                                                minigame_data_table_write_handle,
                                                zones_table_write_handle,
                                                Some(44218),
                                                game_server,
                                            ));
                                            return;
                                        } else {
                                            info!(
                                                "Replacement stage (stage group {}, stage {}) for (stage group {}, stage {}) isn't single-player",
                                                replacement_stage_locator.stage_group_guid,
                                                replacement_stage_locator.stage_guid,
                                                stage_group_guid,
                                                stage_guid
                                            );
                                        }
                                    } else {
                                        info!(
                                            "Couldn't find replacement stage (stage group {}, stage {}) for (stage group {}, stage {})",
                                            replacement_stage_locator.stage_group_guid,
                                            replacement_stage_locator.stage_guid,
                                            stage_group_guid,
                                            stage_guid
                                        );
                                    }
                                }
                            }

                            let leave_result = leave_active_minigame_if_any(
                                LeaveMinigameTarget::Group(matchmaking_group),
                                characters_table_write_handle,
                                minigame_data_table_write_handle,
                                zones_table_write_handle,
                                Some(33781),
                                false,
                                game_server,
                            );
                            match leave_result {
                                Ok(mut leave_broadcasts) => broadcasts.append(&mut leave_broadcasts),
                                Err(err) => info!("Unable to remove timed-out group {:?} from matchmaking: {}", matchmaking_group, err),
                            }
                        })
                    }
                }

                broadcasts
            })
        },
    )
}

pub fn enqueue_tickable_minigames(
    game_server: &GameServer,
    minigames_enqueue: Sender<MinigameMatchmakingGroup>,
) -> usize {
    let minigame_data_lock_enforcer: MinigameDataLockEnforcer = game_server.lock_enforcer().into();
    minigame_data_lock_enforcer.read_minigame_data(|minigame_data_table_read_handle| {
        let count = minigame_data_table_read_handle
            .keys_by_index1(SharedMinigameDataTickableIndex::Tickable)
            .fold(0, |count, minigame_group| {
                minigames_enqueue
                    .send(minigame_group)
                    .expect("Minigame tick channel disconnected");
                count + 1
            });
        MinigameDataLockRequest {
            read_guids: Vec::new(),
            write_guids: Vec::new(),
            minigame_data_consumer: move |_, _, _, _| count,
        }
    })
}

pub fn tick_minigame(
    game_server: &GameServer,
    now: Instant,
    minigame_group: MinigameMatchmakingGroup,
) -> Vec<Broadcast> {
    let minigame_data_lock_enforcer: MinigameDataLockEnforcer = game_server.lock_enforcer().into();
    minigame_data_lock_enforcer.read_minigame_data(|_| MinigameDataLockRequest {
        read_guids: Vec::new(),
        write_guids: vec![minigame_group],
        minigame_data_consumer: |_, _, mut minigame_data_write, _| {
            let mut broadcasts = Vec::new();
            for minigame_data in minigame_data_write.values_mut() {
                broadcasts.append(&mut minigame_data.tick(now));
            }

            broadcasts
        },
    })
}

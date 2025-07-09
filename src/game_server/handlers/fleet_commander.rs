use std::{
    collections::BTreeMap,
    fmt::Display,
    time::{Duration, Instant},
};

use enum_iterator::{all, Sequence};
use num_enum::TryFromPrimitive;
use rand::Rng;
use serde::Serializer;

use crate::game_server::{
    handlers::{
        character::MinigameStatus, guid::GuidTableIndexer, lock_enforcer::CharacterTableReadHandle,
        minigame::MinigameTimer, unique_guid::player_guid,
    },
    packets::{
        minigame::{FlashPayload, MinigameHeader},
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, ProcessPacketError, ProcessPacketErrorType,
};

const BOARD_SIZE: u8 = 15;

const SHIP_PLACEMENT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Sequence, TryFromPrimitive)]
#[repr(u8)]
enum FleetCommanderShipSize {
    Two = 0,
    Three = 1,
    Four = 2,
    Five = 3,
}

impl FleetCommanderShipSize {
    pub const fn value(&self) -> u8 {
        match *self {
            FleetCommanderShipSize::Two => 2,
            FleetCommanderShipSize::Three => 3,
            FleetCommanderShipSize::Four => 4,
            FleetCommanderShipSize::Five => 5,
        }
    }

    pub const fn max_per_player(&self) -> u8 {
        match *self {
            FleetCommanderShipSize::Two => 2,
            FleetCommanderShipSize::Three => 2,
            FleetCommanderShipSize::Four => 2,
            FleetCommanderShipSize::Five => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FleetCommanderShip {
    row: u8,
    col: u8,
    flipped: bool,
    size: FleetCommanderShipSize,
    hits: u8,
}

impl FleetCommanderShip {
    pub fn contains(&self, row: u8, col: u8) -> bool {
        if self.flipped {
            col == self.col && row >= self.row && row < row.saturating_add(self.size.value())
        } else {
            row == self.row && col >= self.col && col < col.saturating_add(self.size.value())
        }
    }

    pub fn hit(&mut self) {
        self.hits = self.hits.saturating_add(1)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
enum FleetCommanderPowerup {
    Square = 0,
    Scatter = 1,
    Homing = 2,
}

impl Display for FleetCommanderPowerup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.serialize_u8(*self as u8)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
enum FleetCommanderPlayerReadiness {
    Ready,
    #[default]
    Unready,
}

#[derive(Clone, Debug, Default)]
struct FleetCommanderPlayerState {
    readiness: FleetCommanderPlayerReadiness,
    ships: Vec<FleetCommanderShip>,
    powerups: [u8; 3],
    score: i32,
}

impl FleetCommanderPlayerState {
    pub fn readiness(&self) -> FleetCommanderPlayerReadiness {
        self.readiness
    }

    pub fn add_ship(
        &mut self,
        sender: u32,
        player_index: u8,
        new_ship_size: FleetCommanderShipSize,
        flipped: bool,
        row: u8,
        col: u8,
    ) -> Result<(), ProcessPacketError> {
        let max_coord = BOARD_SIZE - new_ship_size.value();
        if (flipped && row >= max_coord) || (!flipped && col >= max_coord) {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} (index {}) sent a place ship payload (size: {:?}, flipped: {}, row: {}, col: {}) for Fleet Commander, but the ship is out of bounds ({:?})", sender, player_index, new_ship_size, flipped, row, col, self)));
        }

        self.ships.push(FleetCommanderShip {
            row,
            col,
            flipped,
            size: new_ship_size,
            hits: 0,
        });

        let mut counts = BTreeMap::new();
        for ship in self.ships.iter() {
            counts
                .entry(ship.size)
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }

        let mut readiness = FleetCommanderPlayerReadiness::Ready;
        for size in all::<FleetCommanderShipSize>() {
            let ships_of_size = counts.get(&size).cloned().unwrap_or(0);

            if ships_of_size < size.max_per_player() {
                readiness = FleetCommanderPlayerReadiness::Unready;
            } else if ships_of_size > size.max_per_player() {
                // Remove the new ship we just added
                self.ships.pop();
                return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} (index {}) sent a place ship payload (size: {:?}, flipped: {}, row: {}, col: {}) for Fleet Commander, but they already have the maximum number of ships of that size placed ({:?})", sender, player_index, new_ship_size, flipped, row, col, self)));
            }
        }

        self.readiness = readiness;

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FleetCommanderTurn {
    Player1 = 0,
    Player2 = 1,
}

impl Display for FleetCommanderTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.serialize_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum FleetCommanderPlacementState {
    WaitingForConnection,
    WaitingForPlacement { timer: MinigameTimer },
    Done,
}

#[derive(Clone, Debug)]
enum FleetCommanderGameState {
    WaitingForPlayersReady {
        ship_placement_timers: [FleetCommanderPlacementState; 2],
    },
    WaitingForMove {
        timer: MinigameTimer,
    },
    ProcessingMove {
        turn_duration: Duration,
        animations_complete: [bool; 2],
    },
    GameOver,
}

#[derive(Clone, Debug)]
pub struct FleetCommanderGame {
    player1: u32,
    player2: Option<u32>,
    player_states: [FleetCommanderPlayerState; 2],
    turn: FleetCommanderTurn,
    state: FleetCommanderGameState,
    stage_guid: i32,
    stage_group_guid: i32,
}

impl FleetCommanderGame {
    pub fn new(player1: u32, player2: Option<u32>, stage_guid: i32, stage_group_guid: i32) -> Self {
        let turn = if rand::thread_rng().gen_bool(0.5) {
            FleetCommanderTurn::Player1
        } else {
            FleetCommanderTurn::Player2
        };

        FleetCommanderGame {
            player1,
            player2,
            player_states: Default::default(),
            turn,
            state: FleetCommanderGameState::WaitingForPlayersReady {
                ship_placement_timers: [
                    FleetCommanderPlacementState::WaitingForConnection,
                    FleetCommanderPlacementState::WaitingForConnection,
                ],
            },
            stage_guid,
            stage_group_guid,
        }
    }

    pub fn connect(
        &mut self,
        sender: u32,
        characters_table_read_handle: &CharacterTableReadHandle,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if sender != self.player1 && Some(sender) != self.player2 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} sent a connect payload for Fleet Commander, but they aren't one of the game's players ({:?})", sender, self)));
        }

        let Some(name1) = characters_table_read_handle.index2(player_guid(self.player1)) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Fleet Commander player 1 with GUID {} is missing or has no name ({:?})",
                    self.player1, self
                ),
            ));
        };

        let name2 = match self.player2 {
            Some(player2_guid) => characters_table_read_handle
                .index2(player_guid(player2_guid))
                .ok_or(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Fleet Commander player 2 with GUID {} is missing or has no name ({:?})",
                        player2_guid, self
                    ),
                ))?,
            None => &"".to_string(),
        };

        let FleetCommanderGameState::WaitingForPlayersReady {
            ship_placement_timers,
        } = &mut self.state
        else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {} tried to connect to Fleet Commander, but the game has already started ({:?})",
                    sender,
                    self
                ),
            ));
        };

        let player_index = (sender != self.player1) as usize;
        if !matches!(
            ship_placement_timers[player_index],
            FleetCommanderPlacementState::WaitingForConnection
        ) {
            return Ok(Vec::new());
        }

        ship_placement_timers[player_index] = FleetCommanderPlacementState::WaitingForPlacement {
            timer: MinigameTimer::new_with_event(SHIP_PLACEMENT_TIMEOUT),
        };

        let mut broadcasts = vec![Broadcast::Single(
            sender,
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: "OnServerReadyMsg".to_string(),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!(
                            "OnAssignPlayerIndexMsg\t{}",
                            player_index
                        ),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnAddPlayerMsg\t0\t{}\t{}\tfalse", name1, self.player1),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!(
                            "OnAddPlayerMsg\t1\t{}\t{}\t{}",
                            name2,
                            self.player2.unwrap_or(0),
                            self.is_ai_match()
                        ),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!(
                            "OnLevelDataMsg\t{size},{size},{len2_ships},{len3_ships},{len4_ships},{len5_ships}",
                            size = BOARD_SIZE,
                            len2_ships = FleetCommanderShipSize::Two.max_per_player(),
                            len3_ships = FleetCommanderShipSize::Three.max_per_player(),
                            len4_ships = FleetCommanderShipSize::Four.max_per_player(),
                            len5_ships = FleetCommanderShipSize::Five.max_per_player()
                        ),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnStartShipPlacementMsg\t{}", SHIP_PLACEMENT_TIMEOUT.as_millis()),
                    },
                })
            ],
        )];

        for player_index in 0..=1 {
            broadcasts.append(&mut self.broadcast_powerup_quantity(player_index));
        }

        Ok(broadcasts)
    }

    pub fn place_ship(
        &mut self,
        sender: u32,
        ship_size: u8,
        flipped: bool,
        row: u8,
        col: u8,
        player_index: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let ship_size = FleetCommanderShipSize::try_from(ship_size).map_err(|_| ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} sent a place ship payload for Force Connection, but ship size {} isn't valid ({:?})", sender, ship_size, self)))?;

        if (player_index == 0 && sender == self.player1)
            || (player_index == 1
                && (Some(sender) == self.player2 || (self.is_ai_match() && sender == self.player1)))
        {
            self.player_states[player_index as usize].add_ship(
                sender,
                player_index,
                ship_size,
                flipped,
                row,
                col,
            )?;
        } else {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} sent a place ship payload for Force Connection, but they aren't one of the game's players ({:?})", sender, self)));
        }

        if self.player_states[0].readiness() == FleetCommanderPlayerReadiness::Unready
            || self.player_states[1].readiness() == FleetCommanderPlayerReadiness::Unready
        {
            return Ok(Vec::new());
        }

        let mut broadcasts = vec![Broadcast::Multi(
            self.list_recipients(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: "OnStartGameMsg".to_string(),
                },
            })],
        )];

        // TODO: start turn
        Ok(broadcasts)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        match &mut self.state {
            FleetCommanderGameState::WaitingForPlayersReady {
                ship_placement_timers,
            } => {
                let mut broadcasts = Vec::new();
                broadcasts.append(&mut FleetCommanderGame::tick_ship_placement_timer(
                    &mut ship_placement_timers[0],
                    0,
                    self.player1,
                    now,
                    self.stage_group_guid,
                    self.stage_guid,
                ));
                if let Some(player2) = self.player2 {
                    broadcasts.append(&mut FleetCommanderGame::tick_ship_placement_timer(
                        &mut ship_placement_timers[1],
                        1,
                        player2,
                        now,
                        self.stage_group_guid,
                        self.stage_guid,
                    ));
                }

                broadcasts
            }
            FleetCommanderGameState::WaitingForMove { timer } => Vec::new(), //self.switch_turn(),
            _ => Vec::new(),
        }
    }

    pub fn pause_or_resume(
        &mut self,
        player: u32,
        pause: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if player != self.player1 && Some(player) != self.player2 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to pause or resume (pause: {}) the game for player {}, who is not playing this instance of Fleet Commander ({:?})", pause, player, self)));
        };

        if !self.is_ai_match() {
            return Ok(Vec::new());
        }

        match &mut self.state {
            FleetCommanderGameState::WaitingForPlayersReady {
                ship_placement_timers,
            } => {
                for placement_state in ship_placement_timers.iter_mut() {
                    if let FleetCommanderPlacementState::WaitingForPlacement { timer } =
                        placement_state
                    {
                        timer.pause_or_resume(pause);
                    }
                }
            }
            FleetCommanderGameState::WaitingForMove { timer } => timer.pause_or_resume(pause),
            _ => (),
        }

        Ok(Vec::new())
    }

    pub fn remove_player(
        &self,
        player: u32,
        minigame_status: &mut MinigameStatus,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = if player == self.player1 {
            0
        } else if Some(player) == self.player2 {
            1
        } else {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to remove player {}, who is not playing this instance of Fleet Commander ({:?})", player, self)));
        };

        /*minigame_status.game_won = self.ships_remaining[player_index] >= MATCHES_TO_WIN;
        minigame_status.total_score = self.score[player_index];
        minigame_status.score_entries.push(ScoreEntry {
            entry_text: "".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Total,
            score_count: self.score[player_index],
            score_max: 0,
            score_points: 0,
        });*/

        Ok(Vec::new())
    }

    fn is_ai_match(&self) -> bool {
        self.player2.is_none()
    }

    fn is_ai_player(&self, player_index: u8) -> bool {
        player_index == 1 && self.player2.is_none()
    }

    fn list_recipients(&self) -> Vec<u32> {
        let mut recipients = vec![self.player1];
        if let Some(player2) = self.player2 {
            recipients.push(player2);
        }

        recipients
    }

    fn tick_ship_placement_timer(
        placement_state: &mut FleetCommanderPlacementState,
        player_index: u8,
        player: u32,
        now: Instant,
        stage_group_guid: i32,
        stage_guid: i32,
    ) -> Vec<Broadcast> {
        let FleetCommanderPlacementState::WaitingForPlacement { timer } = placement_state else {
            return Vec::new();
        };

        if timer.paused() {
            return Vec::new();
        }

        timer.update_timer(now);
        if timer.time_until_next_event(now).is_zero() {
            *placement_state = FleetCommanderPlacementState::Done;
            vec![Broadcast::Single(
                player,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid,
                            sub_op_code: -1,
                            stage_group_guid,
                        },
                        payload: format!("OnTriggerAutoPlaceShipMsg\t{}", player_index),
                    },
                })],
            )]
        } else {
            Vec::new()
        }
    }

    /*fn check_turn(
        &self,
        sender: u32,
        player_index: u8,
        turn_time: Instant,
    ) -> Result<(), ProcessPacketError> {
        let is_valid_for_player = match player_index {
            0 => self.turn == FleetCommanderTurn::Player1 && sender == self.player1,
            1 => self.turn == FleetCommanderTurn::Player2 && ((sender == self.player1 && self.is_ai_match()) || (Some(sender) == self.player2)),
            _ => return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} tried to make a move in Fleet Commander, but the player index {} isn't valid ({:?})", sender, player_index, self)))
        };

        if !is_valid_for_player {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} (index {}) tried to make a move in Fleet Commander, but it isn't their turn ({:?})", sender, player_index, self)));
        }

        if self.time_until_next_event(turn_time).is_zero() {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {} (index {}) tried to make a move in Fleet Commander, but their turn expired ({:?})", sender, player_index, self)));
        }

        if self.state != FleetCommanderGameState::WaitingForMove {
            let log_level = if self.is_ai_player(player_index) {
                // There's a known issue with the AI player attempting to use a powerup and drop a piece at the same time.
                // Don't return an error to avoid log spam.
                LogLevel::Debug
            } else {
                LogLevel::Info
            };
            return Err(ProcessPacketError::new_with_log_level(ProcessPacketErrorType::ConstraintViolated, format!("Player {} (index {}) tried to make a move in Fleet Commander, but the state is {:?} instead of waiting for a move ({:?})", sender, player_index, self.state, self), log_level));
        }

        Ok(())
    }

    fn switch_turn(&mut self) -> Vec<Broadcast> {
        let mut broadcasts = Vec::new();
        if let FleetCommanderGameState::Matching {
            turn_duration: time_left_in_turn,
        } = self.state
        {
            let score_from_turn_time = time_left_in_turn.as_secs() as i32 * 5;
            self.score[self.turn as usize] =
                self.score[self.turn as usize].saturating_add(score_from_turn_time);
            broadcasts.push(Broadcast::Multi(
                self.list_recipients(),
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!(
                            "OnScoreFromTimeMsg\t{}\t{}",
                            self.turn, score_from_turn_time
                        ),
                    },
                })],
            ));
        }

        self.state = FleetCommanderGameState::WaitingForMove;
        self.turn = match self.turn {
            FleetCommanderTurn::Player1 => FleetCommanderTurn::Player2,
            FleetCommanderTurn::Player2 => FleetCommanderTurn::Player1,
        };

        self.schedule_event(Duration::from_secs(TURN_TIME_SECONDS as u64));

        broadcasts.push(Broadcast::Multi(
            self.list_recipients(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!("OnStartPlayerTurnMsg\t{}\t{}", self.turn, TURN_TIME_SECONDS),
                },
            })],
        ));
        broadcasts
    }

    fn process_matches(&mut self) -> Vec<Broadcast> {
        let (player1_matches, player2_matches, empty_slots) = self.player_states.process_matches();
        if empty_slots.is_empty() {
            self.check_for_winner()
                .unwrap_or_else(|| self.switch_turn())
        } else {
            let mut broadcasts = Vec::new();

            for player1_match_len in player1_matches {
                broadcasts.append(&mut self.process_match(player1_match_len, 0));
            }

            for player2_match_len in player2_matches {
                broadcasts.append(&mut self.process_match(player2_match_len, 1));
            }

            self.schedule_event(Duration::from_millis(500));

            broadcasts.push(Broadcast::Multi(
                self.list_recipients(),
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnSlotsToClearMsg\t{}", EmptySlots(empty_slots)),
                    },
                })],
            ));

            broadcasts
        }
    }

    fn process_match(&mut self, match_length: u8, player_index: u8) -> Vec<Broadcast> {
        self.ships_remaining[player_index as usize] = self.ships_remaining[player_index as usize].saturating_add(1);

        let value_per_space = 100 + (match_length - MIN_MATCH_LENGTH) as i32 * 50;
        let score_from_match = value_per_space * match_length as i32;
        self.score[player_index as usize] =
            self.score[player_index as usize].saturating_add(score_from_match);

        let mut broadcasts = Vec::new();

        if match_length > MIN_MATCH_LENGTH {
            let new_powerup = if thread_rng().gen_range(0.0..=1.0) > 0.66 {
                FleetCommanderPowerup::Swap
            } else {
                FleetCommanderPowerup::Delete
            };

            self.powerups[player_index as usize][new_powerup as usize] =
                self.powerups[player_index as usize][new_powerup as usize].saturating_add(1);

            broadcasts.push(Broadcast::Multi(
                self.list_recipients(),
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnPowerUpAddedMsg\t{}\t{}", player_index, new_powerup),
                    },
                })],
            ));
            broadcasts.append(&mut self.broadcast_powerup_quantity(player_index));
        }

        broadcasts.push(Broadcast::Multi(
            self.list_recipients(),
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnAddScoreMsg\t{}\t{}", player_index, score_from_match),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnAddMatchMsg\t{}", player_index),
                    },
                }),
            ],
        ));

        broadcasts
    }

    fn handle_move(&mut self, turn_time: Instant, sleep_time: Duration) -> Vec<Broadcast> {
        self.state = FleetCommanderGameState::Matching {
            turn_duration: self.time_until_next_event(turn_time),
        };

        self.schedule_event(sleep_time);
        self.broadcast_powerup_quantity(self.turn as u8)
    }

    fn check_for_winner(&mut self) -> Option<Vec<Broadcast>> {
        let player1_won = self.ships_remaining[0] >= MATCHES_TO_WIN;
        let player2_won = self.ships_remaining[1] >= MATCHES_TO_WIN;

        if !player1_won && !player2_won {
            return None;
        }

        let mut broadcasts = Vec::new();
        self.state = FleetCommanderGameState::GameOver;
        broadcasts.append(&mut self.broadcast_game_result(self.player1, player1_won));
        if let Some(player2) = self.player2 {
            broadcasts.append(&mut self.broadcast_game_result(player2, player2_won));
        }

        Some(broadcasts)
    }

    fn broadcast_game_result(&self, player: u32, won: bool) -> Vec<Broadcast> {
        if won {
            vec![Broadcast::Single(
                player,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: "OnGameWonMsg".to_string(),
                    },
                })],
            )]
        } else {
            vec![Broadcast::Single(
                player,
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: "OnGameLostMsg".to_string(),
                    },
                })],
            )]
        }
    }*/

    fn broadcast_powerup_quantity(&self, player_index: u8) -> Vec<Broadcast> {
        vec![Broadcast::Multi(
            self.list_recipients(),
            vec![/*GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!(
                        "OnPowerUpRemainingMsg\t{}\t{},{}",
                        player_index,
                        self.powerups[player_index as usize][FleetCommanderPowerup::Swap as usize],
                        self.powerups[player_index as usize]
                            [FleetCommanderPowerup::Delete as usize]
                    ),
                },
            })*/],
        )]
    }
}

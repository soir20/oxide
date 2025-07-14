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
        minigame::{FlashPayload, MinigameHeader, ScoreEntry, ScoreType},
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, ProcessPacketError, ProcessPacketErrorType,
};

const BOARD_SIZE: u8 = 15;

const SHIP_PLACEMENT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
enum FleetCommanderDifficulty {
    Easy,
    Medium,
    Hard,
}

impl FleetCommanderDifficulty {
    pub fn turn_timeout(&self) -> Duration {
        match *self {
            FleetCommanderDifficulty::Easy => Duration::from_secs(15),
            FleetCommanderDifficulty::Medium => Duration::from_secs(12),
            FleetCommanderDifficulty::Hard => Duration::from_secs(8),
        }
    }

    pub fn score_per_turn_second_remaining(&self) -> i32 {
        match *self {
            FleetCommanderDifficulty::Easy => 3,
            FleetCommanderDifficulty::Medium => 5,
            FleetCommanderDifficulty::Hard => 10,
        }
    }

    pub fn score_per_hit(&self, powerup: Option<FleetCommanderPowerup>) -> i32 {
        match powerup {
            Some(FleetCommanderPowerup::Square) => match *self {
                FleetCommanderDifficulty::Easy => 450,
                FleetCommanderDifficulty::Medium => 540,
                FleetCommanderDifficulty::Hard => todo!(),
            },
            Some(FleetCommanderPowerup::Scatter) => match *self {
                FleetCommanderDifficulty::Easy => 375,
                FleetCommanderDifficulty::Medium => 450,
                FleetCommanderDifficulty::Hard => 525,
            },
            Some(FleetCommanderPowerup::Homing) => todo!(),
            None => 1000,
        }
    }

    pub fn default_powerup_quantity(&self) -> u8 {
        match *self {
            FleetCommanderDifficulty::Easy => 3,
            FleetCommanderDifficulty::Medium => 2,
            FleetCommanderDifficulty::Hard => 1,
        }
    }
}

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

impl Display for FleetCommanderShipSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.serialize_u8(*self as u8)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FleetCommanderShip {
    row: u8,
    col: u8,
    vertical: bool,
    size: FleetCommanderShipSize,
    hits: u8,
}

impl FleetCommanderShip {
    pub fn contains(&self, hit_row: u8, hit_col: u8) -> bool {
        if self.vertical {
            hit_col == self.col
                && hit_row >= self.row
                && hit_row < self.row.saturating_add(self.size.value())
        } else {
            hit_row == self.row
                && hit_col >= self.col
                && hit_col < self.col.saturating_add(self.size.value())
        }
    }

    pub fn hit(&mut self) {
        self.hits = self.hits.saturating_add(1)
    }

    pub fn destroyed(&self) -> bool {
        self.hits >= self.size.value()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
enum FleetCommanderPowerup {
    Square = 0,
    Scatter = 1,
    Homing = 2,
}

impl FleetCommanderPowerup {
    pub fn ship_size(&self) -> FleetCommanderShipSize {
        match *self {
            FleetCommanderPowerup::Square => FleetCommanderShipSize::Four,
            FleetCommanderPowerup::Scatter => FleetCommanderShipSize::Three,
            FleetCommanderPowerup::Homing => FleetCommanderShipSize::Two,
        }
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum FleetCommanderHitResult {
    Miss,
    ShipDamaged,
    ShipDestroyed(FleetCommanderShip),
}

type HitArrayItem = u32;

#[derive(Clone, Debug, Default)]
struct FleetCommanderPlayerState {
    readiness: FleetCommanderPlayerReadiness,
    ships: Vec<FleetCommanderShip>,
    hits: [HitArrayItem;
        (BOARD_SIZE as u32 * BOARD_SIZE as u32).div_ceil(HitArrayItem::BITS) as usize],
    powerups: [u8; 3],
    score: i32,
}

impl FleetCommanderPlayerState {
    pub fn new(difficulty: FleetCommanderDifficulty) -> Self {
        FleetCommanderPlayerState {
            readiness: Default::default(),
            ships: Default::default(),
            hits: Default::default(),
            powerups: [difficulty.default_powerup_quantity(); 3],
            score: 0,
        }
    }

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
        if (flipped && row > max_coord) || (!flipped && col > max_coord) {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) sent a place ship payload (size: {new_ship_size:?}, flipped: {flipped}, row: {row}, col: {col}) for Fleet Commander, but the ship is out of bounds ({self:?})")));
        }

        self.ships.push(FleetCommanderShip {
            row,
            col,
            vertical: flipped,
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
                return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) sent a place ship payload (size: {new_ship_size:?}, flipped: {flipped}, row: {row}, col: {col}) for Fleet Commander, but they already have the maximum number of ships of that size placed ({self:?})")));
            }
        }

        self.readiness = readiness;

        Ok(())
    }

    pub fn hit(&mut self, row: u8, col: u8) -> Result<FleetCommanderHitResult, ProcessPacketError> {
        if row >= BOARD_SIZE || col >= BOARD_SIZE {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Space ({row}, {col}) is outside the board"),
            ));
        }

        let index = row as usize * BOARD_SIZE as usize + col as usize;
        let hit_section = index / HitArrayItem::BITS as usize;
        let hit_index_in_section = index % HitArrayItem::BITS as usize;

        let previously_hit = (self.hits[hit_section] >> hit_index_in_section) & 1 != 0;
        if previously_hit {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Space ({row}, {col}) was already hit"),
            ));
        }

        self.hits[hit_section] |= 1 << hit_index_in_section;

        for ship_index in 0..self.ships.len() {
            let ship = &mut self.ships[ship_index];
            if ship.contains(row, col) {
                ship.hit();
                if ship.destroyed() {
                    return Ok(FleetCommanderHitResult::ShipDestroyed(
                        self.ships.swap_remove(ship_index),
                    ));
                } else {
                    return Ok(FleetCommanderHitResult::ShipDamaged);
                }
            }
        }

        Ok(FleetCommanderHitResult::Miss)
    }

    pub fn use_powerup(
        &mut self,
        powerup: FleetCommanderPowerup,
    ) -> Result<(), ProcessPacketError> {
        self.powerups[powerup as usize] = self.powerups[powerup as usize]
            .checked_sub(1)
            .ok_or_else(|| {
                ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!("Player has no more powerups of type {powerup}"),
                )
            })?;
        Ok(())
    }

    pub fn add_score(&mut self, score: i32) -> i32 {
        self.score = self.score.saturating_add(score);
        self.score
    }

    pub fn score(&self) -> i32 {
        self.score
    }

    pub fn lost(&self) -> bool {
        self.ships.is_empty()
    }

    pub fn ships(&self) -> impl Iterator<Item = &FleetCommanderShip> {
        self.ships.iter()
    }

    pub fn powerups_remaining(&self, powerup: FleetCommanderPowerup) -> u8 {
        self.powerups[powerup as usize]
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
        time_left_in_turn: Duration,
        animations_complete: [bool; 2],
    },
    GameOver,
}

#[derive(Clone, Debug)]
pub struct FleetCommanderGame {
    difficulty: FleetCommanderDifficulty,
    player1: u32,
    player2: Option<u32>,
    player_states: [FleetCommanderPlayerState; 2],
    turn: FleetCommanderTurn,
    state: FleetCommanderGameState,
    stage_guid: i32,
    stage_group_guid: i32,
}

impl FleetCommanderGame {
    pub fn new(
        raw_difficulty: u32,
        player1: u32,
        player2: Option<u32>,
        stage_guid: i32,
        stage_group_guid: i32,
    ) -> Self {
        let turn = if rand::thread_rng().gen_bool(0.5) {
            FleetCommanderTurn::Player1
        } else {
            FleetCommanderTurn::Player2
        };

        let difficulty = match raw_difficulty {
            2 => FleetCommanderDifficulty::Medium,
            3 => FleetCommanderDifficulty::Hard,
            _ => FleetCommanderDifficulty::Easy,
        };

        FleetCommanderGame {
            difficulty,
            player1,
            player2,
            player_states: [
                FleetCommanderPlayerState::new(difficulty),
                FleetCommanderPlayerState::new(difficulty),
            ],
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
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} sent a connect payload for Fleet Commander, but they aren't one of the game's players ({self:?})")));
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
                        "Fleet Commander player 2 with GUID {player2_guid} is missing or has no name ({self:?})"
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
                    "Player {sender} tried to connect to Fleet Commander, but the game has already started ({self:?})"
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
                            "OnAssignPlayerIndexMsg\t{player_index}"
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
        let ship_size = FleetCommanderShipSize::try_from(ship_size).map_err(|_| ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} sent a place ship payload for Force Connection, but ship size {ship_size} isn't valid ({self:?})")))?;

        let is_valid_for_player = match player_index {
            0 => sender == self.player1,
            1 => (sender == self.player1 && self.is_ai_match()) || Some(sender) == self.player2,
            _ => return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} tried to place a ship in Fleet Commander, but the player index {player_index} isn't valid ({self:?})")))
        };
        if !is_valid_for_player {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} sent a place ship payload for Force Connection, but they aren't one of the game's players ({self:?})")));
        }

        self.player_states[player_index as usize].add_ship(
            sender,
            player_index,
            ship_size,
            flipped,
            row,
            col,
        )?;

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

        broadcasts.append(&mut self.switch_turn());
        Ok(broadcasts)
    }

    pub fn hit(
        &mut self,
        sender: u32,
        row: u8,
        col: u8,
        player_index: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        self.hit_single_space(sender, row, col, player_index, None)
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
            FleetCommanderGameState::WaitingForMove { timer } => {
                if timer.paused() {
                    return Vec::new();
                }

                timer.update_timer(now);
                if !timer.time_until_next_event(now).is_zero() {
                    return Vec::new();
                }

                self.switch_turn()
            }
            _ => Vec::new(),
        }
    }

    pub fn pause_or_resume(
        &mut self,
        player: u32,
        pause: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if player != self.player1 && Some(player) != self.player2 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to pause or resume (pause: {pause}) the game for player {player}, who is not playing this instance of Fleet Commander ({self:?})")));
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
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to remove player {player}, who is not playing this instance of Fleet Commander ({self:?})")));
        };

        minigame_status.game_won = !self.player_states[player_index].lost()
            && self.player_states[(player_index + 1) % 2].lost();
        minigame_status.total_score = self.player_states[player_index].score();
        minigame_status.score_entries.push(ScoreEntry {
            entry_text: "".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Total,
            score_count: self.player_states[player_index].score(),
            score_max: 0,
            score_points: 0,
        });

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

    fn hit_single_space(
        &mut self,
        sender: u32,
        row: u8,
        col: u8,
        player_index: u8,
        powerup_if_used: Option<FleetCommanderPowerup>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let turn_time = Instant::now();
        let time_left_in_turn = self.check_turn(sender, player_index, turn_time)?;

        let target_index = match self.turn {
            FleetCommanderTurn::Player1 => 1,
            FleetCommanderTurn::Player2 => 0,
        };

        let (did_damage, destroyed_ship) =
            match self.player_states[target_index as usize].hit(row, col)? {
                FleetCommanderHitResult::Miss => (false, None),
                FleetCommanderHitResult::ShipDamaged => (true, None),
                FleetCommanderHitResult::ShipDestroyed(ship) => (true, Some(ship)),
            };

        if did_damage {
            self.player_states[player_index as usize]
                .add_score(self.difficulty.score_per_hit(powerup_if_used));
        }

        // TODO: add powerup
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
                    payload: format!(
                        "OnGridBombedMsg\t{target_index}\t{row}\t{col}\t{did_damage}\t-1"
                    ),
                },
            })],
        )];

        if let Some(ship) = destroyed_ship {
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
                            "OnShipDestroyedMsg\t{target_index}\t{}\t{}\t{}\t{}",
                            ship.size, ship.row, ship.col, ship.vertical
                        ),
                    },
                })],
            ));
        }

        self.state = FleetCommanderGameState::ProcessingMove {
            time_left_in_turn,
            animations_complete: [true, true],
        };
        broadcasts.append(&mut self.switch_turn());

        Ok(broadcasts)
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
                        payload: format!("OnTriggerAutoPlaceShipMsg\t{player_index}"),
                    },
                })],
            )]
        } else {
            Vec::new()
        }
    }

    fn check_turn(
        &self,
        sender: u32,
        player_index: u8,
        turn_time: Instant,
    ) -> Result<Duration, ProcessPacketError> {
        let is_valid_for_player = match player_index {
            0 => self.turn == FleetCommanderTurn::Player1 && sender == self.player1,
            1 => self.turn == FleetCommanderTurn::Player2 && ((sender == self.player1 && self.is_ai_match()) || (Some(sender) == self.player2)),
            _ => return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} tried to make a move in Fleet Commander, but the player index {player_index} isn't valid ({self:?})")))
        };

        if !is_valid_for_player {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to make a move in Fleet Commander, but it isn't their turn ({self:?})")));
        }

        let FleetCommanderGameState::WaitingForMove { timer } = &self.state else {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to make a move in Fleet Commander, but the state is {:?} instead of waiting for a move ({self:?})", self.state)));
        };

        let time_left_in_turn = timer.time_until_next_event(turn_time);
        if time_left_in_turn.is_zero() {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to make a move in Fleet Commander, but their turn expired ({self:?})")));
        }

        Ok(time_left_in_turn)
    }

    fn switch_turn(&mut self) -> Vec<Broadcast> {
        let mut broadcasts = Vec::new();
        if let FleetCommanderGameState::ProcessingMove {
            time_left_in_turn, ..
        } = self.state
        {
            let score_from_turn_time = time_left_in_turn.as_secs() as i32
                * self.difficulty.score_per_turn_second_remaining();
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
                            "OnScoreUpdateMsg\t{}\t{}",
                            self.turn,
                            self.player_states[self.turn as usize].add_score(score_from_turn_time)
                        ),
                    },
                })],
            ));
        }

        if let Some(game_result_broadcasts) = self.check_for_winner() {
            return game_result_broadcasts;
        }

        self.state = FleetCommanderGameState::WaitingForMove {
            timer: MinigameTimer::new_with_event(self.difficulty.turn_timeout()),
        };
        self.turn = match self.turn {
            FleetCommanderTurn::Player1 => FleetCommanderTurn::Player2,
            FleetCommanderTurn::Player2 => FleetCommanderTurn::Player1,
        };

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
                        "OnStartPlayerTurnMsg\t{}\t{}",
                        self.turn,
                        self.difficulty.turn_timeout().as_millis()
                    ),
                },
            })],
        ));
        broadcasts
    }

    fn check_for_winner(&mut self) -> Option<Vec<Broadcast>> {
        let player1_lost = self.player_states[0].lost();
        let player2_lost = self.player_states[1].lost();

        if !player1_lost && !player2_lost {
            return None;
        }

        let mut broadcasts = Vec::new();
        self.state = FleetCommanderGameState::GameOver;
        broadcasts.append(&mut self.broadcast_game_result(self.player1, 0, player1_lost));
        if let Some(player2) = self.player2 {
            broadcasts.append(&mut self.broadcast_game_result(player2, 1, player2_lost));
        }

        Some(broadcasts)
    }

    fn broadcast_game_result(&self, player: u32, player_index: u8, lost: bool) -> Vec<Broadcast> {
        if lost {
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
        } else {
            let mut winner_ship_packets = Vec::new();
            for ship in self.player_states[player_index as usize].ships() {
                let other_player_index = (player_index as usize + 1) % 2;
                winner_ship_packets.push(GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!(
                            "OnWinnerShipsRemaining\t{other_player_index}\t{}\t{}\t{}\t{}",
                            ship.size, ship.row, ship.col, ship.vertical
                        ),
                    },
                }))
            }

            vec![
                Broadcast::Single(
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
                ),
                Broadcast::Multi(self.list_recipients(), winner_ship_packets),
            ]
        }
    }

    fn broadcast_powerup_quantity(&self, player_index: u8) -> Vec<Broadcast> {
        vec![Broadcast::Multi(
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
                        "OnPowerUpRemainingMsg\t{player_index}\t{},{},{}",
                        self.player_states[player_index as usize]
                            .powerups_remaining(FleetCommanderPowerup::Square),
                        self.player_states[player_index as usize]
                            .powerups_remaining(FleetCommanderPowerup::Scatter),
                        self.player_states[player_index as usize]
                            .powerups_remaining(FleetCommanderPowerup::Homing),
                    ),
                },
            })],
        )]
    }
}

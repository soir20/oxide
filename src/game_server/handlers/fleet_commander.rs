use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    time::{Duration, Instant},
};

use enum_iterator::{all, Sequence};
use num_enum::TryFromPrimitive;
use rand::{thread_rng, Rng};
use serde::Serializer;

use crate::{
    debug,
    game_server::{
        handlers::{
            character::MinigameStatus, guid::GuidTableIndexer,
            lock_enforcer::CharacterTableReadHandle, minigame::MinigameTimer,
            unique_guid::player_guid,
        },
        packets::{
            minigame::{FlashPayload, MinigameHeader, ScoreEntry, ScoreType},
            tunnel::TunneledPacket,
            GamePacket,
        },
        Broadcast, ProcessPacketError, ProcessPacketErrorType,
    },
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

    pub fn score_per_hit(&self, powerup_if_used: Option<FleetCommanderPowerup>) -> i32 {
        let base_score = match *self {
            FleetCommanderDifficulty::Easy => 500,
            FleetCommanderDifficulty::Medium => 600,
            FleetCommanderDifficulty::Hard => 700,
        };

        match powerup_if_used {
            Some(FleetCommanderPowerup::Square) => base_score * 9 / 10,
            Some(FleetCommanderPowerup::Scatter) => base_score * 3 / 4,
            Some(FleetCommanderPowerup::Homing) => base_score / 2,
            None => base_score,
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
    pub fn value(&self) -> u8 {
        match *self {
            FleetCommanderShipSize::Two => 2,
            FleetCommanderShipSize::Three => 3,
            FleetCommanderShipSize::Four => 4,
            FleetCommanderShipSize::Five => 5,
        }
    }

    pub fn max_per_player(&self) -> u8 {
        match *self {
            FleetCommanderShipSize::Two => 2,
            FleetCommanderShipSize::Three => 2,
            FleetCommanderShipSize::Four => 2,
            FleetCommanderShipSize::Five => 1,
        }
    }

    pub fn score_from_destruction(&self) -> i32 {
        match *self {
            FleetCommanderShipSize::Two => 500,
            FleetCommanderShipSize::Three => 400,
            FleetCommanderShipSize::Four => 300,
            FleetCommanderShipSize::Five => 250,
        }
    }

    pub fn powerup(&self) -> Option<FleetCommanderPowerup> {
        match *self {
            FleetCommanderShipSize::Two => Some(FleetCommanderPowerup::Homing),
            FleetCommanderShipSize::Three => Some(FleetCommanderPowerup::Scatter),
            FleetCommanderShipSize::Four => Some(FleetCommanderPowerup::Square),
            FleetCommanderShipSize::Five => None,
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

    pub fn coordinates(&self) -> impl Iterator<Item = (u8, u8)> + use<'_> {
        let base_value = if self.vertical { self.col } else { self.row };

        (base_value..(base_value.saturating_add(self.size.value()))).map(|value| {
            if self.vertical {
                (self.row, value)
            } else {
                (value, self.col)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum FleetCommanderHitResult {
    Miss(Option<FleetCommanderPowerup>),
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
    findable_powerups: Vec<(u8, u8, FleetCommanderPowerup)>,
    powerups: [u8; 3],
    powerups_enabled: [bool; 3],
    score: i32,
}

impl FleetCommanderPlayerState {
    pub fn new(difficulty: FleetCommanderDifficulty) -> Self {
        FleetCommanderPlayerState {
            readiness: Default::default(),
            ships: Default::default(),
            hits: Default::default(),
            findable_powerups: Default::default(),
            powerups: [difficulty.default_powerup_quantity(); 3],
            powerups_enabled: [true; 3],
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

        if self.readiness == FleetCommanderPlayerReadiness::Ready {
            self.findable_powerups = self.generate_findable_powerups();
        }

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
            debug!("Space ({row}, {col}) was already hit");
            return Ok(FleetCommanderHitResult::Miss(None));
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

        let mut findable_powerup = None;
        for findable_powerup_index in 0..self.findable_powerups.len() {
            let (powerup_row, powerup_col, powerup) =
                self.findable_powerups[findable_powerup_index];

            if row == powerup_row && col == powerup_col {
                findable_powerup = Some(powerup);
                self.findable_powerups.swap_remove(findable_powerup_index);
                break;
            }
        }

        Ok(FleetCommanderHitResult::Miss(findable_powerup))
    }

    pub fn can_use_powerup(&mut self, powerup: FleetCommanderPowerup) -> bool {
        self.powerups[powerup as usize] > 0 && self.powerups_enabled[powerup as usize]
    }

    pub fn use_powerup(&mut self, powerup: FleetCommanderPowerup) {
        self.powerups[powerup as usize] = self.powerups[powerup as usize].saturating_sub(1);
    }

    pub fn disable_powerup(&mut self, powerup: FleetCommanderPowerup) {
        self.powerups[powerup as usize] = 0;
        self.powerups_enabled[powerup as usize] = false;
    }

    pub fn add_powerup_if_enabled(&mut self, powerup: FleetCommanderPowerup) {
        if self.powerups_enabled[powerup as usize] {
            self.powerups[powerup as usize] = self.powerups[powerup as usize].saturating_add(1);
        }
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

    fn generate_findable_powerups(&self) -> Vec<(u8, u8, FleetCommanderPowerup)> {
        let available_powerups = [
            FleetCommanderPowerup::Square,
            FleetCommanderPowerup::Square,
            FleetCommanderPowerup::Square,
            FleetCommanderPowerup::Scatter,
            FleetCommanderPowerup::Scatter,
            FleetCommanderPowerup::Scatter,
            FleetCommanderPowerup::Homing,
            FleetCommanderPowerup::Homing,
        ];

        let total_coords_len = BOARD_SIZE as usize * BOARD_SIZE as usize;
        let disallowed_coords: BTreeSet<usize> = self
            .ships()
            .flat_map(|ship| ship.coordinates())
            .map(|(row, col)| row as usize * BOARD_SIZE as usize + col as usize)
            .collect();

        // Every coordinate must either be in the selectable range or outside it, so we need to
        // replace any disallowed coords that happen to fall in the selectable range with
        // the allowed coords that must have fallen in the unselectable range.
        // len(selectable but disallowed coords) = len(unselectable but allowed coords)
        let selectable_coords_len = total_coords_len - disallowed_coords.len();
        let mut unselectable_allowed_coords: Vec<usize> = (selectable_coords_len..total_coords_len)
            .filter(|coord| !disallowed_coords.contains(coord))
            .collect();

        rand::seq::index::sample(
            &mut thread_rng(),
            selectable_coords_len,
            available_powerups.len(),
        )
        .into_iter()
        .map(|coord| {
            if disallowed_coords.contains(&coord) {
                unselectable_allowed_coords
                    .pop()
                    .expect("Not enough replacement coordinates in Force Connection")
            } else {
                coord
            }
        })
        .zip(available_powerups)
        .map(|(coord, powerup): (usize, FleetCommanderPowerup)| {
            (
                (coord / BOARD_SIZE as usize) as u8,
                (coord % BOARD_SIZE as usize) as u8,
                powerup,
            )
        })
        .collect()
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
    recipients: Vec<u32>,
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

        let mut recipients = vec![player1];
        if let Some(player2) = player2 {
            recipients.push(player2);
        }

        FleetCommanderGame {
            difficulty,
            player1,
            player2,
            recipients,
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
        let ship_size = FleetCommanderShipSize::try_from(ship_size).map_err(|_| ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} sent a place ship payload for Fleet Commander, but ship size {ship_size} isn't valid ({self:?})")))?;

        let is_valid_for_player = match player_index {
            0 => sender == self.player1,
            1 => (sender == self.player1 && self.is_ai_match()) || Some(sender) == self.player2,
            _ => return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} tried to place a ship in Fleet Commander, but the player index {player_index} isn't valid ({self:?})")))
        };
        if !is_valid_for_player {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} sent a place ship payload for Fleet Commander, but they aren't one of the game's players ({self:?})")));
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
            self.recipients.clone(),
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
        let turn_time = Instant::now();
        let time_left_in_turn = self.check_turn(sender, player_index, turn_time)?;

        let mut broadcasts = self.hit_single_space(row, col, None)?;

        self.state = FleetCommanderGameState::ProcessingMove {
            time_left_in_turn,
            animations_complete: [true, true],
        };
        broadcasts.append(&mut self.switch_turn());

        Ok(broadcasts)
    }

    pub fn use_powerup(
        &mut self,
        sender: u32,
        row: u8,
        col: u8,
        attacker_index: u8,
        powerup: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let powerup = FleetCommanderPowerup::try_from(powerup).map_err(|_| ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} use powerup payload for Fleet Commander, but powerup {powerup} isn't valid ({self:?})")))?;

        let turn_time = Instant::now();
        let time_left_in_turn = self.check_turn(sender, attacker_index, turn_time)?;

        if !self.player_states[attacker_index as usize].can_use_powerup(powerup) {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} (index {attacker_index}) has no more powerups of type {powerup}"
                ),
            ));
        }

        let target_index = (attacker_index + 1) % 2;
        let mut broadcasts = vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!("OnTogglePowerUpModeMsg\t{attacker_index}\t1"),
                },
            })],
        )];

        broadcasts.append(&mut self.hit_single_space(row, col, Some(powerup))?);

        broadcasts.push(Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!("OnUsePowerUpMsg\t{target_index}\t{powerup}\t{row}\t{col}"),
                },
            })],
        ));

        self.player_states[attacker_index as usize].use_powerup(powerup);

        self.state = FleetCommanderGameState::ProcessingMove {
            time_left_in_turn,
            animations_complete: [false; 2],
        };

        Ok(broadcasts)
    }

    pub fn complete_powerup_animation(
        &mut self,
        sender: u32,
        player_index: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let is_valid_for_player = match player_index {
            0 => sender == self.player1,
            1 => (sender == self.player1 && self.is_ai_match()) || Some(sender) == self.player2,
            _ => return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} tried to complete a powerup animation in Fleet Commander, but the player index {player_index} isn't valid ({self:?})")))
        };

        if !is_valid_for_player {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to complete a powerup animation in Fleet Commander, but a different player has that index ({self:?})")));
        }

        let FleetCommanderGameState::ProcessingMove {
            animations_complete,
            ..
        } = &mut self.state
        else {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} tried to complete a powerup animation in Fleet Commander, but no move is being processed ({self:?})")));
        };

        animations_complete[player_index as usize] = true;

        if animations_complete.iter().all(|complete| *complete) {
            let mut broadcasts = vec![Broadcast::Multi(
                self.recipients.clone(),
                vec![GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnTogglePowerUpModeMsg\t{}\t0", self.turn),
                    },
                })],
            )];
            broadcasts.append(&mut self.switch_turn());
            Ok(broadcasts)
        } else {
            Ok(Vec::new())
        }
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

    fn hit_single_space(
        &mut self,
        row: u8,
        col: u8,
        powerup_if_used: Option<FleetCommanderPowerup>,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let target_index = match self.turn {
            FleetCommanderTurn::Player1 => 1,
            FleetCommanderTurn::Player2 => 0,
        };
        let attacker_index = self.turn as usize;

        let (did_damage, destroyed_ship, powerup_to_add) =
            match self.player_states[target_index].hit(row, col)? {
                FleetCommanderHitResult::Miss(powerup_to_add) => (false, None, powerup_to_add),
                FleetCommanderHitResult::ShipDamaged => (true, None, None),
                FleetCommanderHitResult::ShipDestroyed(ship) => (true, Some(ship), None),
            };

        if did_damage {
            self.player_states[attacker_index]
                .add_score(self.difficulty.score_per_hit(powerup_if_used));
        }

        if let Some(powerup) = powerup_to_add {
            self.player_states[attacker_index].add_powerup_if_enabled(powerup);
        }

        let mut broadcasts = vec![Broadcast::Multi(
            self.recipients.clone(),
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!(
                        "OnGridBombedMsg\t{target_index}\t{row}\t{col}\t{did_damage}\t{}",
                        powerup_to_add.map(|powerup| powerup as i32).unwrap_or(-1)
                    ),
                },
            })],
        )];

        if let Some(ship) = destroyed_ship {
            self.player_states[attacker_index].add_score(ship.size.score_from_destruction());

            if !self.player_states[target_index]
                .ships()
                .any(|live_ship| live_ship.size == ship.size)
            {
                if let Some(associated_powerup) = ship.size.powerup() {
                    self.player_states[target_index].disable_powerup(associated_powerup);
                    broadcasts.push(Broadcast::Multi(
                        self.recipients.clone(),
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: FlashPayload {
                                header: MinigameHeader {
                                    stage_guid: self.stage_guid,
                                    sub_op_code: -1,
                                    stage_group_guid: self.stage_group_guid,
                                },
                                payload: format!(
                                    "OnPowerUpDisabledMsg\t{target_index}\t{associated_powerup}"
                                ),
                            },
                        })],
                    ));
                    broadcasts.append(&mut self.broadcast_powerup_quantity(target_index as u8));
                }
            }

            broadcasts.push(Broadcast::Multi(
                self.recipients.clone(),
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
            1 => self.turn == FleetCommanderTurn::Player2 && ((sender == self.player1 && self.is_ai_match()) || Some(sender) == self.player2),
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
                self.recipients.clone(),
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
            self.recipients.clone(),
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
            let mut winner_ship_packets = Vec::new();
            let other_player_index = (player_index as usize + 1) % 2;
            for ship in self.player_states[other_player_index].ships() {
                winner_ship_packets.push(GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!(
                            "OnWinnerShipsRemaining\t{player_index}\t{}\t{}\t{}\t{}",
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
                            payload: "OnGameLostMsg".to_string(),
                        },
                    })],
                ),
                Broadcast::Multi(self.recipients.clone(), winner_ship_packets),
            ]
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
                        payload: "OnGameWonMsg".to_string(),
                    },
                })],
            )]
        }
    }

    fn broadcast_powerup_quantity(&self, player_index: u8) -> Vec<Broadcast> {
        vec![Broadcast::Multi(
            self.recipients.clone(),
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

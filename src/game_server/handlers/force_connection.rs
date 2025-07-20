use std::{
    fmt::{Display, Write},
    time::{Duration, Instant},
};

use rand::{thread_rng, Rng};
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
    Broadcast, LogLevel, ProcessPacketError, ProcessPacketErrorType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForceConnectionPiece {
    Wall = 0,
    Empty = 1,
    Player1 = 2,
    Player2 = 3,
}

impl Display for ForceConnectionPiece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.serialize_u8(*self as u8)?;
        Ok(())
    }
}

const BOARD_SIZE: u8 = 10;
const MIN_MATCH_LENGTH: u8 = 4;
const TURN_TIME: Duration = Duration::from_secs(20);
const MATCHES_TO_WIN: u8 = 5;

#[derive(Clone, Debug)]
struct ForceConnectionBoard {
    board: [[ForceConnectionPiece; BOARD_SIZE as usize]; BOARD_SIZE as usize],
    next_open_row: [u8; BOARD_SIZE as usize],
    modified_cols: [Option<u8>; BOARD_SIZE as usize],
}

impl ForceConnectionBoard {
    pub const fn new() -> Self {
        let mut board = [[ForceConnectionPiece::Empty; BOARD_SIZE as usize]; BOARD_SIZE as usize];
        let next_open_row = [3u8, 2u8, 1u8, 0u8, 0u8, 0u8, 0u8, 1u8, 2u8, 3u8];

        let corner_indices = [0u8, 9u8, 1u8, 8u8, 2u8, 7u8];

        let mut col_index = 0;
        while col_index < corner_indices.len() {
            let col = corner_indices[col_index];

            let mut row_index = 0;
            while row_index < corner_indices.len() - (col_index / 2) * 2 {
                let row = corner_indices[row_index];
                board[col as usize][row as usize] = ForceConnectionPiece::Wall;

                row_index += 1;
            }

            col_index += 1;
        }

        ForceConnectionBoard {
            board,
            next_open_row,
            modified_cols: [None; BOARD_SIZE as usize],
        }
    }

    pub fn drop_piece(
        &mut self,
        col: u8,
        piece: ForceConnectionPiece,
    ) -> Result<u8, ProcessPacketError> {
        ForceConnectionBoard::check_col_in_bounds(col)?;

        let Some(next_open_row) = self.next_open_row(col) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Cannot drop piece in column {col} that is already full"),
            ));
        };

        self.set_piece(next_open_row, col, piece);

        Ok(next_open_row)
    }

    pub fn swap_pieces(
        &mut self,
        row1: u8,
        col1: u8,
        row2: u8,
        col2: u8,
    ) -> Result<(), ProcessPacketError> {
        ForceConnectionBoard::check_row_in_bounds(row1)?;
        ForceConnectionBoard::check_col_in_bounds(col1)?;
        ForceConnectionBoard::check_row_in_bounds(row2)?;
        ForceConnectionBoard::check_col_in_bounds(col2)?;

        let piece1 = self.piece(row1, col1);
        let piece2 = self.piece(row2, col2);
        if piece1 == ForceConnectionPiece::Empty || piece1 == ForceConnectionPiece::Wall {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Piece 1 at ({row1}, {col1}) must be a player piece but was: {piece1:?}"),
            ));
        }

        if piece2 == ForceConnectionPiece::Empty || piece2 == ForceConnectionPiece::Wall {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Piece 2 at ({row2}, {col2}) must be a player piece but was: {piece2:?}"),
            ));
        }

        if piece1 == piece2 {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Tried to swap identical pieces at ({row1}, {col1}) and ({row2}, {col2}): {piece1:?}"
                ),
            ));
        }

        if row1.abs_diff(row2) > 1 || col1.abs_diff(col2) > 1 {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Tried to swap pieces at ({row1}, {col1}) and ({row2}, {col2}), which are more than 1 row or column apart: {piece1:?}"
                ),
            ));
        }

        self.set_piece(row1, col1, piece2);
        self.set_piece(row2, col2, piece1);

        Ok(())
    }

    pub fn delete_piece_if_matches(
        &mut self,
        row: u8,
        col: u8,
        expected_piece: ForceConnectionPiece,
    ) -> Result<(), ProcessPacketError> {
        ForceConnectionBoard::check_row_in_bounds(row)?;
        ForceConnectionBoard::check_col_in_bounds(col)?;

        let piece = self.piece(row, col);
        if piece != expected_piece {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Piece to remove at ({row}, {col}) was expected to be {expected_piece:?}, but was {piece:?}",
                ),
            ));
        }

        self.set_piece(row, col, ForceConnectionPiece::Empty);
        self.apply_gravity();

        Ok(())
    }

    pub fn process_matches(&mut self) -> (Vec<u8>, Vec<u8>, Vec<(u8, u8)>) {
        let mut player1_matches = Vec::new();
        let mut player2_matches = Vec::new();
        let mut cleared_pieces = Vec::new();

        let modified_cols = self.modified_cols;
        self.modified_cols = [None; BOARD_SIZE as usize];

        for col in 0..BOARD_SIZE {
            let Some(max_row) = modified_cols[col as usize] else {
                continue;
            };
            for row in (0..=max_row).rev() {
                let Some((piece_type, mut match_pieces)) = self.check_match(row, col) else {
                    continue;
                };

                let match_len = match_pieces.len() as u8;
                match piece_type {
                    ForceConnectionPiece::Player1 => player1_matches.push(match_len),
                    ForceConnectionPiece::Player2 => player2_matches.push(match_len),
                    _ => panic!("Found match for non-player piece type {piece_type:?}"),
                }

                for (match_row, match_col) in match_pieces.iter() {
                    self.set_piece(*match_row, *match_col, ForceConnectionPiece::Empty);
                }

                cleared_pieces.append(&mut match_pieces);
            }
        }

        self.apply_gravity();

        (player1_matches, player2_matches, cleared_pieces)
    }

    fn check_row_in_bounds(row: u8) -> Result<(), ProcessPacketError> {
        if row >= BOARD_SIZE {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Row {row} is outside the board"),
            ));
        }

        Ok(())
    }

    fn check_col_in_bounds(col: u8) -> Result<(), ProcessPacketError> {
        if col >= BOARD_SIZE {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Column {col} is outside the board"),
            ));
        }

        Ok(())
    }

    fn piece(&self, row: u8, col: u8) -> ForceConnectionPiece {
        self.board[col as usize][row as usize]
    }

    fn set_piece(&mut self, row: u8, col: u8, piece: ForceConnectionPiece) {
        self.board[col as usize][row as usize] = piece;
        self.modified_cols[col as usize] =
            Some(self.modified_cols[col as usize].unwrap_or(0).max(row));

        let next_open_row = self.next_open_row[col as usize];

        if piece == ForceConnectionPiece::Empty {
            // If we emptied the space where the topmost piece used to be,
            // we need to search for the new next empty space.
            if next_open_row > 0 && row == next_open_row - 1 {
                let mut possible_empty_row = row;
                loop {
                    if self.board[col as usize][possible_empty_row as usize]
                        == ForceConnectionPiece::Empty
                    {
                        self.next_open_row[col as usize] = possible_empty_row;
                    } else {
                        break;
                    }

                    if possible_empty_row == 0 {
                        break;
                    } else {
                        possible_empty_row -= 1;
                    }
                }
            }
        } else {
            // There are two cases:
            // 1) The piece is below the next open row, so the next open row is unchanged.
            // 2) The piece is at or above the next open row, implying all spaces above
            //    are empty. This means the next open row is the row above this piece.
            self.next_open_row[col as usize] = next_open_row.max(row + 1);
        }
    }

    fn next_open_row(&self, col: u8) -> Option<u8> {
        let next_open_row = self.next_open_row[col as usize];
        if next_open_row >= BOARD_SIZE
            || self.piece(next_open_row, col) != ForceConnectionPiece::Empty
        {
            return None;
        }

        Some(next_open_row)
    }

    fn check_match(
        &self,
        origin_row: u8,
        origin_col: u8,
    ) -> Option<(ForceConnectionPiece, Vec<(u8, u8)>)> {
        for (delta_row, delta_col) in [(1, 1), (1, -1), (0, 1), (1, 0)] {
            let (piece, match_pieces) =
                self.check_pattern(origin_row, origin_col, delta_row, delta_col);
            if match_pieces.len() >= MIN_MATCH_LENGTH as usize {
                return Some((piece, match_pieces));
            }
        }

        None
    }

    fn check_pattern(
        &self,
        origin_row: u8,
        origin_col: u8,
        delta_row: i8,
        delta_col: i8,
    ) -> (ForceConnectionPiece, Vec<(u8, u8)>) {
        let mut match_spaces = vec![(origin_row, origin_col)];

        let first_piece = self.piece(origin_row, origin_col);
        if first_piece != ForceConnectionPiece::Player1
            && first_piece != ForceConnectionPiece::Player2
        {
            return (first_piece, Vec::new());
        }

        for direction_coefficient in [1, -1] {
            let adjusted_delta_row = direction_coefficient * delta_row;
            let adjusted_delta_col = direction_coefficient * delta_col;

            let Some(mut cur_row) = origin_row.checked_add_signed(adjusted_delta_row) else {
                continue;
            };
            let Some(mut cur_col) = origin_col.checked_add_signed(adjusted_delta_col) else {
                continue;
            };
            while cur_row < BOARD_SIZE && cur_col < BOARD_SIZE {
                if self.piece(cur_row, cur_col) != first_piece {
                    break;
                }

                match_spaces.push((cur_row, cur_col));

                let Some(new_row) = cur_row.checked_add_signed(adjusted_delta_row) else {
                    break;
                };
                let Some(new_col) = cur_col.checked_add_signed(adjusted_delta_col) else {
                    break;
                };
                cur_row = new_row;
                cur_col = new_col;
            }
        }

        (first_piece, match_spaces)
    }

    fn apply_gravity(&mut self) {
        for col in 0..BOARD_SIZE {
            let mut next_empty_row = None;
            if self.modified_cols[col as usize].is_none() {
                continue;
            }
            for row in 0..self.next_open_row[col as usize] {
                match self.piece(row, col) {
                    ForceConnectionPiece::Wall => next_empty_row = None,
                    ForceConnectionPiece::Empty => next_empty_row = next_empty_row.or(Some(row)),
                    piece => {
                        let Some(empty_row) = next_empty_row else {
                            continue;
                        };

                        self.set_piece(empty_row, col, piece);
                        self.set_piece(row, col, ForceConnectionPiece::Empty);

                        // We already moved all pieces below this downwards, so we can simply check
                        // if the row above the row we filled is empty
                        if empty_row < BOARD_SIZE - 1
                            && self.piece(empty_row + 1, col) == ForceConnectionPiece::Empty
                        {
                            next_empty_row = Some(empty_row + 1);
                        } else {
                            next_empty_row = None;
                        }
                    }
                }
            }
        }
    }
}

impl Display for ForceConnectionBoard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for row in (0..BOARD_SIZE).rev() {
            for col in 0..BOARD_SIZE {
                f.serialize_u8(self.piece(row, col) as u8)?;
                if row > 0 || col < BOARD_SIZE - 1 {
                    f.serialize_char(',')?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForceConnectionTurn {
    Player1 = 0,
    Player2 = 1,
}

impl Display for ForceConnectionTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.serialize_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForceConnectionGameState {
    WaitingForPlayersReady,
    WaitingForMove,
    Matching { time_left_in_turn: Duration },
    GameOver,
}

struct EmptySlots(Vec<(u8, u8)>);

impl Display for EmptySlots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for index in 0..self.0.len() {
            let (row, col) = self.0[index];
            f.serialize_u8(internal_row_to_external_row(row) * BOARD_SIZE + col)?;
            if index < self.0.len() - 1 {
                f.write_char(',')?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ForceConnectionPowerup {
    Swap = 0,
    Delete = 1,
}

impl Display for ForceConnectionPowerup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.serialize_u8(*self as u8)
    }
}

fn internal_row_to_external_row(internal_row: u8) -> u8 {
    BOARD_SIZE - internal_row - 1
}

fn try_external_row_to_internal_row(external_row: u8) -> Result<u8, ProcessPacketError> {
    if external_row >= BOARD_SIZE {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!("External row {external_row} is outside the board"),
        ));
    }

    Ok(BOARD_SIZE - external_row - 1)
}

#[derive(Clone, Debug)]
pub struct ForceConnectionGame {
    board: ForceConnectionBoard,
    player1: u32,
    player2: Option<u32>,
    recipients: Vec<u32>,
    ready: [bool; 2],
    matches: [u8; 2],
    score: [i32; 2],
    powerups: [[u32; 2]; 2],
    turn: ForceConnectionTurn,
    state: ForceConnectionGameState,
    timer: MinigameTimer,
    stage_guid: i32,
    stage_group_guid: i32,
}

impl ForceConnectionGame {
    pub fn new(player1: u32, player2: Option<u32>, stage_guid: i32, stage_group_guid: i32) -> Self {
        let turn = if rand::thread_rng().gen_bool(0.5) {
            ForceConnectionTurn::Player1
        } else {
            ForceConnectionTurn::Player2
        };

        let mut recipients = vec![player1];
        if let Some(player2) = player2 {
            recipients.push(player2);
        }

        ForceConnectionGame {
            board: ForceConnectionBoard::new(),
            player1,
            player2,
            recipients,
            ready: [false, player2.is_none()],
            matches: [0; 2],
            score: [0; 2],
            powerups: [[1, 2]; 2],
            turn,
            state: ForceConnectionGameState::WaitingForPlayersReady,
            timer: MinigameTimer::new(),
            stage_guid,
            stage_group_guid,
        }
    }

    pub fn connect(
        &self,
        sender: u32,
        characters_table_read_handle: &CharacterTableReadHandle,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if self.state != ForceConnectionGameState::WaitingForPlayersReady {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Player {sender} tried to connect to Force Connection, but the game has already started ({self:?})"
                ),
            ));
        }

        let Some(name1) = characters_table_read_handle.index2(player_guid(self.player1)) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Force Connection player 1 with GUID {} is missing or has no name ({self:?})",
                    self.player1
                ),
            ));
        };

        let name2 = match self.player2 {
            Some(player2_guid) => characters_table_read_handle
                .index2(player_guid(player2_guid))
                .ok_or(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Force Connection player 2 with GUID {player2_guid} is missing or has no name ({self:?})"
                    ),
                ))?,
            None => &"".to_string(),
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
                            "OnLevelDataMsg\t{BOARD_SIZE},{BOARD_SIZE},{}",
                            self.board
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
                            "OnAssignPlayerIndexMsg\t{}",
                            (sender != self.player1) as u8
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
                        payload: format!("OnAddPlayerMsg\t0\t{name1}\t{}\tfalse", self.player1),
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
                            "OnAddPlayerMsg\t1\t{name2}\t{}\t{}",
                            self.player2.unwrap_or(0),
                            self.is_ai_match()
                        ),
                    },
                }),
            ],
        )];

        for player_index in 0..=1 {
            broadcasts.append(&mut self.broadcast_powerup_quantity(player_index));
        }

        Ok(broadcasts)
    }

    pub fn mark_player_ready(&mut self, sender: u32) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if sender == self.player1 {
            if self.ready[0] {
                return Ok(Vec::new());
            }
            self.ready[0] = true;
        } else if Some(sender) == self.player2 {
            if self.ready[1] {
                return Ok(Vec::new());
            }
            self.ready[1] = true;
        } else {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} sent a ready payload for Force Connection, but they aren't one of the game's players ({self:?})")));
        }

        if !self.ready[0] || !self.ready[1] {
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

    pub fn select_column(
        &self,
        sender: u32,
        col: u8,
        player_index: i8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let player_index = if player_index < 0 {
            1
        } else {
            player_index as u8
        };
        self.check_turn(sender, player_index, Instant::now())?;

        if col >= BOARD_SIZE {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to select column {col} in Force Connection, but it isn't a valid column ({self:?})")));
        }

        let recipient = match self.turn {
            ForceConnectionTurn::Player1 => {
                let Some(player2) = self.player2 else {
                    return Ok(Vec::new());
                };

                player2
            }
            ForceConnectionTurn::Player2 => self.player1,
        };

        Ok(vec![Broadcast::Single(
            recipient,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!("OnOtherPlayerSelectNewColumnMsg\t{col}"),
                },
            })],
        )])
    }

    pub fn drop_piece(
        &mut self,
        sender: u32,
        col: u8,
        player_index: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let turn_time = Instant::now();
        self.check_turn(sender, player_index, turn_time)?;
        let row = self.board.drop_piece(
            col,
            match self.turn {
                ForceConnectionTurn::Player1 => ForceConnectionPiece::Player1,
                ForceConnectionTurn::Player2 => ForceConnectionPiece::Player2,
            },
        )?;

        let mut broadcasts = self.handle_move(turn_time, Duration::from_secs(1));
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
                        "OnDropPieceMsg\t{player_index}\t{}\t{col}",
                        internal_row_to_external_row(row)
                    ),
                },
            })],
        ));

        Ok(broadcasts)
    }

    pub fn toggle_powerup(
        &self,
        sender: u32,
        powerup: u8,
        player_index: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        self.check_turn(sender, player_index, Instant::now())?;

        if powerup > 1 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to toggle powerup {powerup} in Force Connection, but it isn't a valid powerup ({self:?})")));
        }

        if self.powerups[player_index as usize][powerup as usize] == 0 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to toggle powerup {powerup} in Force Connection, but they don't have any more of that powerup ({self:?})")));
        }

        let recipient = match self.turn {
            ForceConnectionTurn::Player1 => {
                let Some(player2) = self.player2 else {
                    return Ok(Vec::new());
                };

                player2
            }
            ForceConnectionTurn::Player2 => self.player1,
        };

        Ok(vec![Broadcast::Single(
            recipient,
            vec![GamePacket::serialize(&TunneledPacket {
                unknown1: true,
                inner: FlashPayload {
                    header: MinigameHeader {
                        stage_guid: self.stage_guid,
                        sub_op_code: -1,
                        stage_group_guid: self.stage_group_guid,
                    },
                    payload: format!("OnOtherPlayerToggledPowerUpMsg\t{powerup}"),
                },
            })],
        )])
    }

    pub fn use_swap_powerup(
        &mut self,
        sender: u32,
        row1: u8,
        col1: u8,
        row2: u8,
        col2: u8,
        player_index: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let turn_time = Instant::now();
        self.check_turn(sender, player_index, turn_time)?;

        let internal_row1 = try_external_row_to_internal_row(row1)?;
        let internal_row2 = try_external_row_to_internal_row(row2)?;

        if self.powerups[player_index as usize][ForceConnectionPowerup::Swap as usize] == 0 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to swap pieces ({row1}, {col1}) and ({row2}, {col2}) in Force Connection, but they have no swap powersups ({self:?})")));
        }

        self.powerups[player_index as usize][ForceConnectionPowerup::Swap as usize] -= 1;

        self.board
            .swap_pieces(internal_row1, col1, internal_row2, col2)?;

        let mut broadcasts = self.handle_move(turn_time, Duration::from_millis(500));
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
                        "OnUseLightPowerUpMsg\t{player_index}\t{row1}\t{col1}\t{row2}\t{col2}"
                    ),
                },
            })],
        ));

        Ok(broadcasts)
    }

    pub fn use_delete_powerup(
        &mut self,
        sender: u32,
        row: u8,
        col: u8,
        player_index: u8,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let turn_time = Instant::now();
        self.check_turn(sender, player_index, turn_time)?;

        let internal_row = try_external_row_to_internal_row(row)?;

        if self.powerups[player_index as usize][ForceConnectionPowerup::Delete as usize] == 0 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to delete piece ({row}, {col}) in Force Connection, but they have no delete powersups ({self:?})")));
        }

        self.powerups[player_index as usize][ForceConnectionPowerup::Delete as usize] -= 1;

        self.board.delete_piece_if_matches(
            internal_row,
            col,
            match self.turn {
                ForceConnectionTurn::Player1 => ForceConnectionPiece::Player2,
                ForceConnectionTurn::Player2 => ForceConnectionPiece::Player1,
            },
        )?;

        let mut broadcasts = self.handle_move(turn_time, Duration::from_millis(500));
        broadcasts.push(Broadcast::Multi(
            self.recipients.clone(),
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnUseDarkPowerUpMsg\t{player_index}\t{row}\t{col}"),
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
                            "OnSlotsToClearMsg\t{}",
                            EmptySlots(vec![(internal_row, col)])
                        ),
                    },
                }),
            ],
        ));

        Ok(broadcasts)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Broadcast> {
        if self.timer.paused() {
            return Vec::new();
        }

        self.timer.update_timer(now);
        if !self.timer.time_until_next_event(now).is_zero() {
            return Vec::new();
        }

        match self.state {
            ForceConnectionGameState::WaitingForPlayersReady => Vec::new(),
            ForceConnectionGameState::WaitingForMove => {
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
                            payload: format!("OnTurnTimerExpiredMsg\t{}", self.turn),
                        },
                    })],
                )];

                broadcasts.append(&mut self.switch_turn());

                broadcasts
            }
            ForceConnectionGameState::Matching { .. } => self.process_matches(),
            _ => Vec::new(),
        }
    }

    pub fn pause_or_resume(
        &mut self,
        player: u32,
        pause: bool,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        if player != self.player1 && Some(player) != self.player2 {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to pause or resume (pause: {pause}) the game for player {player}, who is not playing this instance of Force Connection ({self:?})")));
        };

        if !self.is_ai_match() {
            return Ok(Vec::new());
        }

        self.timer.pause_or_resume(pause);
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
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Tried to remove player {player}, who is not playing this instance of Force Connection ({self:?})")));
        };

        minigame_status.game_won = self.matches[player_index] >= MATCHES_TO_WIN;
        minigame_status.total_score = self.score[player_index];
        minigame_status.score_entries.push(ScoreEntry {
            entry_text: "".to_string(),
            icon_set_id: 0,
            score_type: ScoreType::Total,
            score_count: self.score[player_index],
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

    fn check_turn(
        &self,
        sender: u32,
        player_index: u8,
        turn_time: Instant,
    ) -> Result<(), ProcessPacketError> {
        let is_valid_for_player = match player_index {
            0 => self.turn == ForceConnectionTurn::Player1 && sender == self.player1,
            1 => self.turn == ForceConnectionTurn::Player2 && ((sender == self.player1 && self.is_ai_match()) || (Some(sender) == self.player2)),
            _ => return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} tried to make a move in Force Connection, but the player index {player_index} isn't valid ({self:?})")))
        };

        if !is_valid_for_player {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to make a move in Force Connection, but it isn't their turn ({self:?})")));
        }

        if self.timer.time_until_next_event(turn_time).is_zero() {
            return Err(ProcessPacketError::new(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to make a move in Force Connection, but their turn expired ({self:?})")));
        }

        if self.state != ForceConnectionGameState::WaitingForMove {
            let log_level = if self.is_ai_player(player_index) {
                // There's a known issue with the AI player attempting to use a powerup and drop a piece at the same time.
                // Don't return an error to avoid log spam.
                LogLevel::Debug
            } else {
                LogLevel::Info
            };
            return Err(ProcessPacketError::new_with_log_level(ProcessPacketErrorType::ConstraintViolated, format!("Player {sender} (index {player_index}) tried to make a move in Force Connection, but the state is {:?} instead of waiting for a move ({self:?})", self.state), log_level));
        }

        Ok(())
    }

    fn switch_turn(&mut self) -> Vec<Broadcast> {
        let mut broadcasts = Vec::new();
        if let ForceConnectionGameState::Matching { time_left_in_turn } = self.state {
            let score_from_turn_time = time_left_in_turn.as_secs() as i32 * 5;
            self.score[self.turn as usize] =
                self.score[self.turn as usize].saturating_add(score_from_turn_time);
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
                            "OnScoreFromTimeMsg\t{}\t{score_from_turn_time}",
                            self.turn
                        ),
                    },
                })],
            ));
        }

        self.state = ForceConnectionGameState::WaitingForMove;
        self.turn = match self.turn {
            ForceConnectionTurn::Player1 => ForceConnectionTurn::Player2,
            ForceConnectionTurn::Player2 => ForceConnectionTurn::Player1,
        };

        self.timer.schedule_event(TURN_TIME);

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
                        TURN_TIME.as_secs()
                    ),
                },
            })],
        ));
        broadcasts
    }

    fn process_matches(&mut self) -> Vec<Broadcast> {
        let (player1_matches, player2_matches, empty_slots) = self.board.process_matches();
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

            self.timer.schedule_event(Duration::from_millis(500));

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
                        payload: format!("OnSlotsToClearMsg\t{}", EmptySlots(empty_slots)),
                    },
                })],
            ));

            broadcasts
        }
    }

    fn process_match(&mut self, match_length: u8, player_index: u8) -> Vec<Broadcast> {
        self.matches[player_index as usize] = self.matches[player_index as usize].saturating_add(1);

        let value_per_space = 100 + (match_length - MIN_MATCH_LENGTH) as i32 * 50;
        let score_from_match = value_per_space * match_length as i32;
        self.score[player_index as usize] =
            self.score[player_index as usize].saturating_add(score_from_match);

        let mut broadcasts = Vec::new();

        if match_length > MIN_MATCH_LENGTH {
            let new_powerup = if thread_rng().gen_range(0.0..=1.0) > 0.66 {
                ForceConnectionPowerup::Swap
            } else {
                ForceConnectionPowerup::Delete
            };

            self.powerups[player_index as usize][new_powerup as usize] =
                self.powerups[player_index as usize][new_powerup as usize].saturating_add(1);

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
                        payload: format!("OnPowerUpAddedMsg\t{player_index}\t{new_powerup}"),
                    },
                })],
            ));
            broadcasts.append(&mut self.broadcast_powerup_quantity(player_index));
        }

        broadcasts.push(Broadcast::Multi(
            self.recipients.clone(),
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: self.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: self.stage_group_guid,
                        },
                        payload: format!("OnAddScoreMsg\t{player_index}\t{score_from_match}"),
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
                        payload: format!("OnAddMatchMsg\t{player_index}"),
                    },
                }),
            ],
        ));

        broadcasts
    }

    fn handle_move(&mut self, turn_time: Instant, sleep_time: Duration) -> Vec<Broadcast> {
        self.state = ForceConnectionGameState::Matching {
            time_left_in_turn: self.timer.time_until_next_event(turn_time),
        };

        self.timer.schedule_event(sleep_time);
        self.broadcast_powerup_quantity(self.turn as u8)
    }

    fn check_for_winner(&mut self) -> Option<Vec<Broadcast>> {
        let player1_won = self.matches[0] >= MATCHES_TO_WIN;
        let player2_won = self.matches[1] >= MATCHES_TO_WIN;

        if !player1_won && !player2_won {
            return None;
        }

        let mut broadcasts = Vec::new();
        self.state = ForceConnectionGameState::GameOver;
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
                        "OnPowerUpRemainingMsg\t{player_index}\t{},{}",
                        self.powerups[player_index as usize][ForceConnectionPowerup::Swap as usize],
                        self.powerups[player_index as usize]
                            [ForceConnectionPowerup::Delete as usize]
                    ),
                },
            })],
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_server::handlers::force_connection::ForceConnectionPiece::Empty;
    use crate::game_server::handlers::force_connection::ForceConnectionPiece::Player1;
    use crate::game_server::handlers::force_connection::ForceConnectionPiece::Player2;
    use crate::game_server::handlers::force_connection::ForceConnectionPiece::Wall;

    const EMPTY_BOARD: [[ForceConnectionPiece; 10]; 10] = [
        [
            Wall, Wall, Wall, Empty, Empty, Empty, Empty, Wall, Wall, Wall,
        ],
        [
            Wall, Wall, Empty, Empty, Empty, Empty, Empty, Empty, Wall, Wall,
        ],
        [
            Wall, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Wall,
        ],
        [
            Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty,
        ],
        [
            Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty,
        ],
        [
            Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty,
        ],
        [
            Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty,
        ],
        [
            Wall, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Empty, Wall,
        ],
        [
            Wall, Wall, Empty, Empty, Empty, Empty, Empty, Empty, Wall, Wall,
        ],
        [
            Wall, Wall, Wall, Empty, Empty, Empty, Empty, Wall, Wall, Wall,
        ],
    ];

    #[test]
    fn test_drop_col_out_of_bounds() {
        let mut board = ForceConnectionBoard::new();
        assert!(board.drop_piece(10, Player1).is_err());
    }

    #[test]
    fn test_wall_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Wall).unwrap();
        board.drop_piece(3, Wall).unwrap();
        board.drop_piece(3, Wall).unwrap();
        board.drop_piece(3, Wall).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[3][0] = Wall;
        expected_board[3][1] = Wall;
        expected_board[3][2] = Wall;
        expected_board[3][3] = Wall;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 4, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 4, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_length_three_no_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player2).unwrap();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(6, Player2).unwrap();
        board.drop_piece(6, Player2).unwrap();
        board.drop_piece(6, Player2).unwrap();
        board.drop_piece(6, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[3][0] = Player1;
        expected_board[4][0] = Player2;
        expected_board[4][1] = Player1;
        expected_board[6][0] = Player2;
        expected_board[6][1] = Player2;
        expected_board[6][2] = Player2;
        expected_board[6][3] = Player1;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 1, 2, 0, 4, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 1, 2, 0, 4, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_horizontal_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(5, Player1).unwrap();
        board.drop_piece(6, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[3][0] = Player1;
        expected_board[5][0] = Player1;
        expected_board[6][0] = Player1;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 1, 0, 1, 1, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.drop_piece(4, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(0, 4), (0, 5), (0, 6), (0, 3)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                None,
                None,
                None
            ],
            board.modified_cols
        );
    }

    #[test]
    fn test_vertical_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[3][0] = Player1;
        expected_board[3][1] = Player1;
        expected_board[3][2] = Player1;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 3, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.drop_piece(3, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(3, 3), (2, 3), (1, 3), (0, 3)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(3),
                None,
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );
    }

    #[test]
    fn test_diagonal_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(6, Player1).unwrap();
        board.drop_piece(8, Player1).unwrap();
        board.drop_piece(9, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[6][0] = Player1;
        expected_board[8][2] = Player1;
        expected_board[9][3] = Player1;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 1, 1, 3, 4], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.drop_piece(7, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(1, 7), (2, 8), (3, 9), (0, 6)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                None,
                None,
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(3)
            ],
            board.modified_cols
        );
    }

    #[test]
    fn test_mirror_diagonal_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(0, Player1).unwrap();
        board.drop_piece(2, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[0][3] = Player1;
        expected_board[2][1] = Player1;
        expected_board[3][0] = Player1;
        assert_eq!(expected_board, board.board);
        assert_eq!([4, 2, 2, 1, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.drop_piece(1, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(2, 1), (3, 0), (1, 2), (0, 3)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                Some(3),
                Some(2),
                Some(1),
                Some(0),
                None,
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );
    }

    #[test]
    fn test_two_matches_at_once_same_player() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(5, Player1).unwrap();
        board.drop_piece(6, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(3, 3), (2, 3), (1, 3), (0, 3)], empty_slots);

        let mut expected_board = EMPTY_BOARD;
        expected_board[4][0] = Player1;
        expected_board[5][0] = Player1;
        expected_board[6][0] = Player1;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 0, 1, 1, 1, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(3),
                None,
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 0, 1, 1, 1, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_two_matches_at_once_diff_players() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(5, Player1).unwrap();
        board.drop_piece(6, Player1).unwrap();

        board.drop_piece(3, Player2).unwrap();
        board.drop_piece(4, Player2).unwrap();
        board.drop_piece(5, Player2).unwrap();
        board.drop_piece(6, Player2).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert_eq!(vec![4], player2_matches);
        assert_eq!(
            vec![
                (1, 3),
                (1, 4),
                (1, 5),
                (1, 6),
                (0, 3),
                (0, 4),
                (0, 5),
                (0, 6)
            ],
            empty_slots
        );

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(1),
                Some(1),
                Some(1),
                Some(1),
                None,
                None,
                None
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_drop_then_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(0, Player1).unwrap();
        board.drop_piece(2, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();

        board.drop_piece(3, Player2).unwrap();
        board.drop_piece(4, Player2).unwrap();
        board.drop_piece(5, Player2).unwrap();
        board.drop_piece(6, Player2).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[0][3] = Player1;
        expected_board[2][1] = Player1;
        expected_board[3][0] = Player1;
        expected_board[3][1] = Player2;
        expected_board[4][0] = Player2;
        expected_board[5][0] = Player2;
        expected_board[6][0] = Player2;
        assert_eq!(expected_board, board.board);
        assert_eq!([4, 2, 2, 2, 1, 1, 1, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.drop_piece(1, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(2, 1), (3, 0), (1, 2), (0, 3)], empty_slots);

        let mut expected_board = EMPTY_BOARD;
        expected_board[3][0] = Player2;
        expected_board[4][0] = Player2;
        expected_board[5][0] = Player2;
        expected_board[6][0] = Player2;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 1, 1, 1, 1, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                Some(3),
                Some(2),
                Some(1),
                Some(1),
                None,
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert_eq!(vec![4], player2_matches);
        assert_eq!(vec![(0, 3), (0, 4), (0, 5), (0, 6)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                None,
                None,
                None
            ],
            board.modified_cols
        );
    }

    #[test]
    fn test_vertical_match_drop_on_wall() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(2, Player1).unwrap();
        board.drop_piece(2, Player1).unwrap();
        board.drop_piece(2, Player1).unwrap();
        board.drop_piece(2, Player1).unwrap();

        board.drop_piece(2, Player2).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(4, 2), (3, 2), (2, 2), (1, 2)], empty_slots);

        let mut expected_board = EMPTY_BOARD;
        expected_board[2][1] = Player2;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 2, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                Some(5),
                None,
                None,
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 2, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_vertical_match_filled_col_without_walls() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();

        assert!(board.drop_piece(3, Player1).is_err());

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![10], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(
            vec![
                (9, 3),
                (8, 3),
                (7, 3),
                (6, 3),
                (5, 3),
                (4, 3),
                (3, 3),
                (2, 3),
                (1, 3),
                (0, 3)
            ],
            empty_slots
        );

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(9),
                None,
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_vertical_match_filled_col_between_left_wall() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(0, Player1).unwrap();
        board.drop_piece(0, Player1).unwrap();
        board.drop_piece(0, Player1).unwrap();
        board.drop_piece(0, Player1).unwrap();

        assert!(board.drop_piece(0, Player1).is_err());

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(6, 0), (5, 0), (4, 0), (3, 0)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                Some(6),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_vertical_match_filled_col_between_right_wall() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(9, Player1).unwrap();
        board.drop_piece(9, Player1).unwrap();
        board.drop_piece(9, Player1).unwrap();
        board.drop_piece(9, Player1).unwrap();

        assert!(board.drop_piece(9, Player1).is_err());

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(6, 9), (5, 9), (4, 9), (3, 9)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(6)
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_horizontal_match_filled_row_top() {
        let mut board = ForceConnectionBoard::new();
        let mut expected_board = EMPTY_BOARD;
        for i in 0..5 {
            for j in 0..2 {
                for col in 3..7 {
                    let remainder = if i % 2 == 0 { 0 } else { 1 };
                    let is_top_row = i == 4 && j == 1;
                    let piece = if col % 2 == remainder || is_top_row {
                        Player1
                    } else {
                        Player2
                    };
                    board.drop_piece(col, piece).unwrap();
                    if !is_top_row {
                        expected_board[col as usize][i * 2 + j] = piece;
                    }
                }
            }
        }

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(9, 3), (9, 4), (9, 5), (9, 6)], empty_slots);

        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 9, 9, 9, 9, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(9),
                Some(9),
                Some(9),
                Some(9),
                None,
                None,
                None
            ],
            board.modified_cols
        );

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 9, 9, 9, 9, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);
    }

    #[test]
    fn test_delete_mismatch() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(4, Player1).unwrap();
        assert!(board.delete_piece_if_matches(0, 4, Player2).is_err());
    }

    #[test]
    fn test_delete_out_of_bounds_row() {
        let mut board = ForceConnectionBoard::new();
        assert!(board.delete_piece_if_matches(10, 0, Player1).is_err());
    }

    #[test]
    fn test_delete_out_of_bounds_col() {
        let mut board = ForceConnectionBoard::new();
        assert!(board.delete_piece_if_matches(0, 10, Player1).is_err());
    }

    #[test]
    fn test_delete_and_match() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(4, Player2).unwrap();
        board.drop_piece(4, Player1).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[4][0] = Player1;
        expected_board[4][1] = Player1;
        expected_board[4][2] = Player1;
        expected_board[4][3] = Player2;
        expected_board[4][4] = Player1;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 0, 5, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.delete_piece_if_matches(3, 4, Player2).unwrap();
        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert!(player2_matches.is_empty());
        assert_eq!(vec![(3, 4), (2, 4), (1, 4), (0, 4)], empty_slots);

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                None,
                Some(3),
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );
    }

    #[test]
    fn test_swap_out_of_bounds_row1() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player2).unwrap();
        assert!(board.swap_pieces(10, 3, 0, 4).is_err());
    }

    #[test]
    fn test_swap_out_of_bounds_col1() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player2).unwrap();
        assert!(board.swap_pieces(0, 10, 0, 4).is_err());
    }

    #[test]
    fn test_swap_out_of_bounds_row2() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player2).unwrap();
        assert!(board.swap_pieces(0, 3, 10, 4).is_err());
    }

    #[test]
    fn test_swap_out_of_bounds_col2() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player2).unwrap();
        assert!(board.swap_pieces(0, 3, 0, 10).is_err());
    }

    #[test]
    fn test_swap_same_piece() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player1).unwrap();
        assert!(board.swap_pieces(0, 3, 0, 4).is_err());
    }

    #[test]
    fn test_swap_empty() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        assert!(board.swap_pieces(0, 3, 0, 4).is_err());
    }

    #[test]
    fn test_swap_wall() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        assert!(board.swap_pieces(0, 3, 0, 2).is_err());
    }

    #[test]
    fn test_swap_rowss_too_far() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player2).unwrap();
        assert!(board.swap_pieces(0, 3, 2, 3).is_err());
    }

    #[test]
    fn test_swap_cols_too_far() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(5, Player2).unwrap();
        assert!(board.swap_pieces(0, 3, 0, 5).is_err());
    }

    #[test]
    fn test_swap_and_match_diff_rows() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(5, Player1).unwrap();
        board.drop_piece(6, Player2).unwrap();
        board.drop_piece(3, Player2).unwrap();
        board.drop_piece(4, Player2).unwrap();
        board.drop_piece(5, Player1).unwrap();
        board.drop_piece(6, Player2).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[3][0] = Player1;
        expected_board[4][0] = Player1;
        expected_board[5][0] = Player1;
        expected_board[6][0] = Player2;
        expected_board[3][1] = Player2;
        expected_board[4][1] = Player2;
        expected_board[5][1] = Player1;
        expected_board[6][1] = Player2;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 2, 2, 2, 2, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.swap_pieces(0, 6, 1, 5).unwrap();
        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert_eq!(vec![4], player2_matches);
        assert_eq!(
            vec![
                (1, 5),
                (1, 6),
                (1, 4),
                (1, 3),
                (0, 5),
                (0, 6),
                (0, 4),
                (0, 3)
            ],
            empty_slots
        );

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(1),
                Some(1),
                Some(1),
                Some(1),
                None,
                None,
                None
            ],
            board.modified_cols
        );
    }

    #[test]
    fn test_swap_and_match_diff_columns() {
        let mut board = ForceConnectionBoard::new();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player1).unwrap();
        board.drop_piece(3, Player2).unwrap();
        board.drop_piece(4, Player2).unwrap();
        board.drop_piece(4, Player2).unwrap();
        board.drop_piece(4, Player1).unwrap();
        board.drop_piece(4, Player2).unwrap();

        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert!(player1_matches.is_empty());
        assert!(player2_matches.is_empty());
        assert!(empty_slots.is_empty());

        let mut expected_board = EMPTY_BOARD;
        expected_board[3][0] = Player1;
        expected_board[3][1] = Player1;
        expected_board[3][2] = Player1;
        expected_board[3][3] = Player2;
        expected_board[4][0] = Player2;
        expected_board[4][1] = Player2;
        expected_board[4][2] = Player1;
        expected_board[4][3] = Player2;
        assert_eq!(expected_board, board.board);
        assert_eq!([3, 2, 1, 4, 4, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!([None; BOARD_SIZE as usize], board.modified_cols);

        board.swap_pieces(3, 3, 2, 4).unwrap();
        let (player1_matches, player2_matches, empty_slots) = board.process_matches();
        assert_eq!(vec![4], player1_matches);
        assert_eq!(vec![4], player2_matches);
        assert_eq!(
            vec![
                (3, 3),
                (2, 3),
                (1, 3),
                (0, 3),
                (2, 4),
                (3, 4),
                (1, 4),
                (0, 4)
            ],
            empty_slots
        );

        assert_eq!(EMPTY_BOARD, board.board);
        assert_eq!([3, 2, 1, 0, 0, 0, 0, 1, 2, 3], board.next_open_row);
        assert_eq!(
            [
                None,
                None,
                None,
                Some(3),
                Some(3),
                None,
                None,
                None,
                None,
                None
            ],
            board.modified_cols
        );
    }
}

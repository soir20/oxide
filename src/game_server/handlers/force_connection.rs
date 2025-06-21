use std::{fmt::Display, time::Instant};

use rand::Rng;
use serde::Serializer;

use crate::game_server::{
    handlers::{
        character::MinigameStatus, guid::GuidTableIndexer, lock_enforcer::CharacterTableReadHandle,
        unique_guid::player_guid,
    },
    packets::{
        minigame::{FlashPayload, MinigameHeader},
        tunnel::TunneledPacket,
        GamePacket,
    },
    Broadcast, ProcessPacketError, ProcessPacketErrorType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForceConnectionPiece {
    Wall = 0,
    Empty = 1,
    Player1 = 2,
    Player2 = 3,
}

const BOARD_SIZE: u8 = 10;
const MIN_MATCH_LENGTH: u8 = 4;

#[derive(Clone)]
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
    ) -> Result<(), ProcessPacketError> {
        ForceConnectionBoard::check_col_in_bounds(col)?;

        let Some(next_open_row) = self.next_open_row(col) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Cannot drop piece in column {} that is already full", col),
            ));
        };

        self.set_piece(next_open_row, col, piece);

        Ok(())
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
                format!(
                    "Piece 1 at ({}, {}) must be a player piece but was: {:?}",
                    row1, col1, piece1
                ),
            ));
        }

        if piece2 == ForceConnectionPiece::Empty || piece2 == ForceConnectionPiece::Wall {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Piece 2 at ({}, {}) must be a player piece but was: {:?}",
                    row2, col2, piece2
                ),
            ));
        }

        if piece1 == piece2 {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Tried to swap identical pieces at ({}, {}) and ({}, {}): {:?}",
                    row1, col1, row2, col2, piece1
                ),
            ));
        }

        if row1.abs_diff(row2) > 1 || col1.abs_diff(col2) > 1 {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Tried to swap pieces at ({}, {}) and ({}, {}), which are more than 1 row or column apart: {:?}",
                    row1, col1, row2, col2, piece1
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
                    "Piece to remove at ({}, {}) was expected to be {:?}, but was {:?}",
                    row, col, expected_piece, piece
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
                    _ => panic!("Found match for non-player piece type {:?}", piece_type),
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
                format!("Row {} is outside the board", row),
            ));
        }

        Ok(())
    }

    fn check_col_in_bounds(col: u8) -> Result<(), ProcessPacketError> {
        if col >= BOARD_SIZE {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!("Column {} is outside the board", col),
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
        for row in 0..BOARD_SIZE {
            for col in 0..BOARD_SIZE {
                f.serialize_u8(self.piece(row, col) as u8)?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
enum ForceConnectionTurn {
    Player1,
    Player2,
}

#[derive(Clone)]
pub struct ForceConnectionGame {
    board: ForceConnectionBoard,
    player1: u32,
    player2: Option<u32>,
    turn: ForceConnectionTurn,
    turn_start: Instant,
    last_tick: Instant,
    done_matching: bool,
    ready_players: u8,
}

impl ForceConnectionGame {
    pub fn new(player1: u32, player2: Option<u32>) -> Self {
        let turn = if rand::thread_rng().gen_bool(0.5) {
            ForceConnectionTurn::Player1
        } else {
            ForceConnectionTurn::Player2
        };
        ForceConnectionGame {
            board: ForceConnectionBoard::new(),
            player1,
            player2,
            turn,
            turn_start: Instant::now(),
            last_tick: Instant::now(),
            done_matching: true,
            ready_players: 0,
        }
    }

    pub fn connect(
        &mut self,
        sender: u32,
        minigame_status: &MinigameStatus,
        characters_table_read_handle: &CharacterTableReadHandle,
    ) -> Result<Vec<Broadcast>, ProcessPacketError> {
        let Some(name1) = characters_table_read_handle.index2(player_guid(self.player1)) else {
            return Err(ProcessPacketError::new(
                ProcessPacketErrorType::ConstraintViolated,
                format!(
                    "Force Connection player 1 with GUID {} is missing or has no name",
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
                        "Force Connection player 2 with GUID {} is missing or has no name",
                        player2_guid
                    ),
                ))?,
            None => &"".to_string(),
        };
        Ok(vec![Broadcast::Single(
            sender,
            vec![
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: minigame_status.group.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.group.stage_group_guid,
                        },
                        payload: "OnServerReadyMsg".to_string(),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: minigame_status.group.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.group.stage_group_guid,
                        },
                        payload: format!(
                            "OnLevelDataMsg\t{size},{size},{board}",
                            size = BOARD_SIZE,
                            board = self.board
                        ),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: minigame_status.group.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.group.stage_group_guid,
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
                            stage_guid: minigame_status.group.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.group.stage_group_guid,
                        },
                        payload: format!("OnAddPlayerMsg\t0\t{}\t{}\tfalse", name1, self.player1),
                    },
                }),
                GamePacket::serialize(&TunneledPacket {
                    unknown1: true,
                    inner: FlashPayload {
                        header: MinigameHeader {
                            stage_guid: minigame_status.group.stage_guid,
                            sub_op_code: -1,
                            stage_group_guid: minigame_status.group.stage_group_guid,
                        },
                        payload: format!(
                            "OnAddPlayerMsg\t1\t{}\t{}\t{}",
                            name2,
                            self.player2.unwrap_or(0),
                            self.player2.is_none()
                        ),
                    },
                }),
            ],
        )])
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

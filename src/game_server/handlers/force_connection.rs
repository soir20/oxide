use crate::game_server::{ProcessPacketError, ProcessPacketErrorType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForceConnectionPiece {
    Wall,
    Empty,
    Player1,
    Player2,
}

const BOARD_SIZE: u8 = 10;
const OUT_OF_BOUNDS_SIZE: u8 = 3;
const MIN_MATCH_LENGTH: u8 = 4;

pub struct ForceConnectionBoard {
    board: [[ForceConnectionPiece; BOARD_SIZE as usize]; BOARD_SIZE as usize],
    next_open_row: [u8; BOARD_SIZE as usize],
    modified_cols: [u8; BOARD_SIZE as usize],
}

impl ForceConnectionBoard {
    pub const fn new() -> Self {
        let mut board = [[ForceConnectionPiece::Empty; BOARD_SIZE as usize]; BOARD_SIZE as usize];
        let next_open_row = [3u8, 2u8, 1u8, 0u8, 0u8, 0u8, 0u8, 1u8, 2u8, 3u8];

        let corner_indices = [0u8, 1u8, 2u8, 7u8, 8u8, 9u8];

        let mut col_index = 0;
        while col_index < corner_indices.len() {
            let col = corner_indices[col_index];
            col_index += 1;

            let mut row_index = 0;
            while row_index < corner_indices.len() {
                let row = corner_indices[row_index];
                row_index += 1;

                board[col as usize][row as usize] = ForceConnectionPiece::Wall;
            }
        }

        ForceConnectionBoard {
            board,
            next_open_row,
            modified_cols: [0; BOARD_SIZE as usize],
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

    pub fn process_matches(&mut self) -> (Vec<u8>, Vec<u8>, Vec<(u8, u8)>) {
        let mut player1_matches = Vec::new();
        let mut player2_matches = Vec::new();
        let mut cleared_pieces = Vec::new();

        let modified_cols = self.modified_cols;
        self.modified_cols = [0u8; BOARD_SIZE as usize];

        for col in 0..BOARD_SIZE {
            for row in (0..=modified_cols[col as usize]).rev() {
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

        for col in 0..BOARD_SIZE {
            let mut next_empty_row = None;
            for row in 0..=modified_cols[col as usize] {
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

        (player1_matches, player2_matches, cleared_pieces)
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
        self.modified_cols[col as usize] = self.modified_cols[col as usize].max(row);

        let next_open_row = self.next_open_row[col as usize];

        if piece == ForceConnectionPiece::Empty {
            // If we modified emptied the space where the topmost piece used to be,
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

            let mut cur_row = origin_row.saturating_add_signed(adjusted_delta_row);
            let mut cur_col = origin_col.saturating_add_signed(adjusted_delta_col);
            while cur_row < BOARD_SIZE && cur_col < BOARD_SIZE {
                if self.piece(cur_row, cur_col) != first_piece {
                    break;
                }

                match_spaces.push((cur_row, cur_col));

                if (cur_row == 0 && adjusted_delta_row < 0)
                    || (cur_col == 0 && adjusted_delta_col < 0)
                {
                    break;
                }
                cur_row = cur_row.saturating_add_signed(adjusted_delta_row);
                cur_col = cur_col.saturating_add_signed(adjusted_delta_col);
            }
        }

        (first_piece, match_spaces)
    }
}

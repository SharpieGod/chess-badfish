#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Color {
    White,
    Black,
}

use std::{collections::HashMap, fmt, ops::BitOr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChessPiece {
    kind: PieceKind,
    color: Color,
}

struct Board([Option<ChessPiece>; 64]);

impl Board {
    fn from_fen(FEN: String) -> Self {
        let pieces = FEN.split_ascii_whitespace().take(1).collect::<String>();
        let mut board: HashMap<u8, ChessPiece> = HashMap::new();
        let mut board_index: u8 = 0;

        let mut piecesMap = HashMap::new();
        piecesMap.insert('P', CPB::Pawn | CPB::White);
        piecesMap.insert('N', CPB::Knight | CPB::White);
        piecesMap.insert('B', CPB::Bishop | CPB::White);
        piecesMap.insert('R', CPB::Rook | CPB::White);
        piecesMap.insert('K', CPB::King | CPB::White);
        piecesMap.insert('Q', CPB::Queen | CPB::White);
        piecesMap.insert('p', CPB::Pawn | CPB::Black);
        piecesMap.insert('n', CPB::Knight | CPB::Black);
        piecesMap.insert('b', CPB::Bishop | CPB::Black);
        piecesMap.insert('r', CPB::Rook | CPB::Black);
        piecesMap.insert('k', CPB::King | CPB::Black);
        piecesMap.insert('q', CPB::Queen | CPB::Black);
        for c in pieces.chars() {
            if let Some(n) = c.to_digit(10) {
                board_index += n as u8;
                continue;
            }

            board.insert(board_index, *piecesMap.get(&c).unwrap());
            board_index += 1;
        }
        Self(board)
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for board_index in 0..64 {
            self.0
                .get(&board_index)
                .unwrap_or(&(CPB::White | CPB::Black));
        }
        write!(f, "")
    }
}

fn main() {
    let starting_board = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string();

    let board = Board::from_fen(starting_board);
    println!("{}", board);
}

enum ChessPiece {
    Pawn = 0,   // 000
    Knight = 1, // 001
    Bishop = 2, // 010
    Rook = 3,   // 011
    Queen = 4,  // 100
    King = 5,   // 101

    White = 8,
    Black = 16,
}

use std::{collections::HashMap, ops::BitOr};

use ChessPiece as CP;

impl BitOr for CP {
    type Output = u8;

    fn bitor(self, rhs: Self) -> Self::Output {
        self as u8 | rhs as u8
    }
}
struct Board {
    board: HashMap<u8, u8>,
}
impl Board {
    fn from_fen(FEN: String) -> Self {
        let pieces = FEN.split_ascii_whitespace().take(1).collect::<String>();
        let mut board = HashMap::new();
        let mut board_index = 0;

        let mut piecesMap = HashMap::new();
        piecesMap.insert("P", CP::Pawn | CP::White);
        piecesMap.insert("N", CP::Knight | CP::White);
        piecesMap.insert("B", CP::Bishop | CP::White);
        piecesMap.insert("R", CP::Rook | CP::White);
        piecesMap.insert("K", CP::King | CP::White);
        piecesMap.insert("Q", CP::Queen | CP::White);

        piecesMap.insert("p", CP::Pawn | CP::Black);
        piecesMap.insert("n", CP::Knight | CP::White);
        piecesMap.insert("b", CP::Bishop | CP::White);
        piecesMap.insert("r", CP::Rook | CP::White);
        piecesMap.insert("k", CP::King | CP::White);
        piecesMap.insert("q", CP::Queen | CP::White);
        for c in pieces.chars() {
            if let Some(n) = c.to_digit(10) {
                board_index += n;
                continue;
            }

            board_index += 1;
        }
        Self { board }
    }
}
fn main() {
    let starting_board = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string();

    let board = Board::from_fen(starting_board);
}

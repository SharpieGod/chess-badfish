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

use std::{
    collections::HashMap,
    fmt::{self, Display},
    io,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChessPiece {
    kind: PieceKind,
    color: Color,
}

impl ChessPiece {
    fn decode_fen(s: char) -> Option<Self> {
        let color = if s.is_uppercase() {
            Color::White
        } else {
            Color::Black
        };

        let kind = match s.to_ascii_lowercase() {
            'p' => Some(PieceKind::Pawn),
            'n' => Some(PieceKind::Knight),
            'b' => Some(PieceKind::Bishop),
            'r' => Some(PieceKind::Rook),
            'k' => Some(PieceKind::King),
            'q' => Some(PieceKind::Queen),
            _ => None,
        };

        if kind.is_none() {
            return None;
        }

        return Some(Self {
            kind: kind.unwrap(),
            color,
        });
    }
    fn encode_fen(&self) -> char {
        let mut c = match self.kind {
            PieceKind::Pawn => 'p',
            PieceKind::Bishop => 'b',
            PieceKind::Knight => 'n',
            PieceKind::Rook => 'r',
            PieceKind::King => 'k',
            PieceKind::Queen => 'q',
        };

        if self.color == Color::White {
            c = c.to_ascii_uppercase();
        }

        c
    }
}
struct Board([Option<ChessPiece>; 64]);

impl Board {
    fn from_fen(fen: String) -> Self {
        let pieces = fen.split_ascii_whitespace().take(1).collect::<String>();
        let mut board: [Option<ChessPiece>; 64] = [None; 64];
        let mut board_index: u8 = 0;

        for c in pieces.chars() {
            if c == '/' {
                continue;
            }

            if let Some(n) = c.to_digit(10) {
                board_index += n as u8;
                continue;
            }

            board[board_index as usize] = ChessPiece::decode_fen(c);
            board_index += 1;
        }

        Self(board)
    }

    fn encode_tile(file: u8, rank: u8) -> u8 {
        (7 - rank) * 8 + file
    }
}

impl Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Straigh up copying stockfish
        let mut out = String::new();
        for i in 0..64 {
            if i % 8 == 0 {
                if i > 0 {
                    out.push('\n');
                }
                out.extend("+---+---+---+---+---+---+---+---+".chars());
                out.push('\n');
                out.push('|');
            }

            if let Some(p) = self.0[i] {
                out.push(' ');
                out.push(p.encode_fen());
                out.extend(" |".chars());
            } else {
                out.extend("   |".chars());
            }

            if i % 8 == 7 {
                out.push(' ');
                out.extend((8 - (i / 8)).to_string().chars());
            }
        }
        out.extend("\n+---+---+---+---+---+---+---+---+\n".chars());
        out.extend("  a   b   c   d   e   f   g   h".chars());

        write!(f, "{}", out)
    }
}

struct Game {
    board: Board,
    white_turn: bool,
}

impl Game {
    fn new() -> Self {
        Self {
            board: Board::from_fen(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string(),
            ),
            white_turn: true,
        }
    }

    fn try_move(&mut self, from: u8, to: u8) -> bool {
        let from_piece = self.board.0[from as usize];
        let to_piece = self.board.0[to as usize];

        println!("{:?} {:?}", from_piece, to_piece);

        if from_piece.is_none()
            || from_piece.is_some()
                && (from_piece.unwrap().color == Color::White) != self.white_turn
            || (from_piece.is_some()
                && to_piece.is_some()
                && from_piece.unwrap().color == to_piece.unwrap().color)
        {
            return false;
        }

        // TODO: Actual piece move logic

        self.board.0[from as usize] = None;
        self.board.0[to as usize] = from_piece;
        self.white_turn = !self.white_turn;

        return true;
    }
}
fn clear() {
    print!("\x1B[2J\x1B[1;1H");
}

fn take_input() -> String {
    let mut input = String::new();

    io::stdin().read_line(&mut input).unwrap_or_default();

    input.trim().to_lowercase().to_string()
}

fn main() {
    let mut game = Game::new();

    loop {
        clear();
        println!("{}", game.board);
        println!("{}", game.white_turn);
        let input = take_input();

        if input.starts_with("mv") {
            let parts = input.split_whitespace().collect::<Vec<&str>>();

            if parts.len() != 3 {
                continue;
            }

            let from = parts[1];
            let to = parts[2];

            if from.len() != to.len() || from.len() != 2 {
                continue;
            }

            let files = vec!['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
            let from_file = files
                .iter()
                .position(|&c| c == from.chars().nth(0).unwrap())
                .unwrap() as u8;
            let from_rank = from.chars().nth(1).unwrap().to_digit(10).unwrap_or(0) as u8 - 1;

            let to_file = files
                .iter()
                .position(|&c| c == to.chars().nth(0).unwrap())
                .unwrap() as u8;
            let to_rank = to.chars().nth(1).unwrap().to_digit(10).unwrap_or(0) as u8 - 1;

            let from_tile = Board::encode_tile(from_file, from_rank);
            let to_tile = Board::encode_tile(to_file, to_rank);

            println!("{} {}", from_tile, to_tile);

            game.try_move(from_tile, to_tile);
        }
    }
}

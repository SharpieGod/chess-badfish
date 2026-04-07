use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
    io,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl TryFrom<usize> for PieceKind {
    type Error = ();

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PieceKind::Pawn),
            1 => Ok(PieceKind::Knight),
            2 => Ok(PieceKind::Bishop),
            3 => Ok(PieceKind::Rook),
            4 => Ok(PieceKind::Queen),
            5 => Ok(PieceKind::King),
            _ => Err(()),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Color {
    White,
    Black,
}

impl TryFrom<usize> for Color {
    type Error = ();

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Color::White),
            1 => Ok(Color::Black),
            _ => Err(()),
        }
    }
}

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

// each bit is a square on the board!
#[derive(Clone, Copy)]
struct BitBoard(u64);

impl BitBoard {
    fn break_down(&self) -> HashSet<u8> {
        let mut set = HashSet::new();
        for i in 0..64 {
            if self.0 & (1 << i) != 0 {
                set.insert(i);
            }
        } 

        set
    }    
    fn insert(&mut self, index: u8) {
        self.0 |= 1 << index;
    }

    fn remove(&mut self, index: u8) {
        self.0 &= !(1 << index);
    }

    fn contains(&self, index: u8) -> bool {
        self.0 & (1 << index) != 0
    }
}

impl Display for BitBoard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in 0..8 {
            for file in 0..8 {
                let index = BC::encode_tile(file, rank);

                let c = if self.contains(index) { '#' } else { '/' };

                write!(f, "{} ", c)?;
            }
            writeln!(f)?;
        }

        writeln!(f)?;
        writeln!(f, "{:064b}", self.0)?;

        Ok(())
    }
}

struct BitBoardCollection {
    // 6 pieces, 2 colours
    piece_boards: [[BitBoard; 6]; 2],
    // TODO: pin bitboard, and yeah
}

use BitBoardCollection as BC;

impl BitBoardCollection {
    fn new() -> Self {
        Self {
            piece_boards: [[BitBoard(0); 6]; 2],
        }
    }
    fn get_board(&mut self, piece: &ChessPiece) -> &mut BitBoard {
        &mut self.piece_boards[piece.color as usize][piece.kind as usize]
    }

    fn insert(&mut self, index: u8, piece: &ChessPiece) {
        self.get_board(piece).insert(index);
    }

    fn remove(&mut self, index: u8, piece: &ChessPiece) {
        self.get_board(piece).remove(index);
    }

    fn contains(&mut self, index: u8, piece: &ChessPiece) -> bool {
        self.get_board(piece).contains(index)
    }

    fn occupied_color(&self, color: Color) -> BitBoard {
        self.piece_boards[color as usize]
            .iter()
            .fold(BitBoard(0), |acc, b| BitBoard(acc.0 | b.0))
    }

    // All occupied squares
    fn occupied(&self) -> BitBoard {
        BitBoard(self.occupied_color(Color::White).0 | self.occupied_color(Color::Black).0)
    }

    fn piece_at_index(&self, index: u8) -> Option<ChessPiece> {
        for c in 0..2 {
            for k in 0..6 {
                if self.piece_boards[c][k].contains(index) {
                    return Some(ChessPiece {
                        kind: k.try_into().unwrap(),
                        color: c.try_into().unwrap(),
                    });
                }
            }
        }

        return None;
    }

    fn from_fen(fen: &str) -> Self {
        let pieces = fen.split_ascii_whitespace().take(1).collect::<String>();
        let mut board_c = Self::new();
        let mut rank = 7;
        let mut file = 0;

        for c in pieces.chars() {
            if c == '/' {
                file = 0;
                rank -= 1;
                continue;
            }

            if let Some(n) = c.to_digit(10) {
                file += n as u8;
                continue;
            }

            board_c.insert(
                BC::encode_tile(file, rank),
                &ChessPiece::decode_fen(c).unwrap(),
            );
            file += 1
        }

        board_c
    }

    fn encode_tile(file: u8, rank: u8) -> u8 {
        ((7 - rank) * 8 + file)
    }

    fn decode_tile(tile: u8) -> (u8, u8) {
        (tile as u8 % 8, 7 - (tile as u8 / 8))
    }

    fn pawn_moves(&self, index: u8) -> HashSet<u8> {
        let (file, rank) = BitBoardCollection::decode_tile(index);
        let rank_mask = BitBoard(0x0101010101010101);

        HashSet::new()
    }
}

impl Display for BitBoardCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in 0..8 {
            writeln!(f, "+---+---+---+---+---+---+---+---+")?;
            write!(f, "|")?;
            for file in 0..8 {
                if let Some(p) =
                    self.piece_at_index(BitBoardCollection::encode_tile(file, 7 - rank))
                {
                    write!(f, " {} |", p.encode_fen())?;
                } else {
                    // empty
                    write!(f, "   |")?;
                }
            }
            writeln!(f)?;
        }
        writeln!(f, "+---+---+---+---+---+---+---+---+")?;

        Ok(())
    }
}

// Basically just FEN
struct Game {
    board_collection: BitBoardCollection,
    white_turn: bool,
    en_passant_square: Option<u8>,
}

impl Game {
    fn new() -> Self {
        Self {
            board_collection: BitBoardCollection::from_fen(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            ),
            white_turn: true,
            en_passant_square: None,
        }
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
        for c in 0..2 {
            for k in 0..6 {
                let piece = ChessPiece {
                    color: c.try_into().unwrap(),
                    kind: k.try_into().unwrap(),
                };

                println!(
                    "{}\n{}",
                    piece.encode_fen(),
                    game.board_collection.get_board(&piece)
                );
            }
        }

        println!("{}", game.white_turn);
        println!("{}", game.board_collection);
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

            let from_tile = BC::encode_tile(from_file, from_rank);
            let to_tile = BC::encode_tile(to_file, to_rank);

            println!("{} {}", from_tile, to_tile);
        }
    }
}

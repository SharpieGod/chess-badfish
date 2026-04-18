use crate::consts::*;
use crate::tables::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PieceKind {
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

impl PieceKind {
    pub fn value(&self) -> i32 {
        MG_PIECE_VALUES[*self as usize]
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn pawn_forward(&self) -> i16 {
        match self {
            Color::White => 8,
            Color::Black => -8,
        }
    }
    pub fn pawn_start_rank(&self) -> RangeInclusive<u8> {
        match self {
            Color::White => 8..=15,
            Color::Black => 48..=55,
        }
    }
    pub fn pawn_promo_rank(&self) -> RangeInclusive<u8> {
        match self {
            Color::White => 56..=63,
            Color::Black => 0..=7,
        }
    }
    pub fn kind(&self, kind: PieceKind) -> ChessPiece {
        ChessPiece { kind, color: *self }
    }
}

impl Not for Color {
    type Output = Color;

    fn not(self) -> Self::Output {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
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
pub struct ChessPiece {
    pub kind: PieceKind,
    pub color: Color,
}

impl ChessPiece {
    pub fn decode_fen(s: char) -> Option<Self> {
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
    pub fn encode_fen(&self) -> char {
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
pub struct BitBoard(pub u64);

impl BitBoard {
    pub fn insert(&mut self, index: u8) {
        self.0 |= 1 << index;
    }

    pub fn remove(&mut self, index: u8) {
        self.0 &= !(1 << index);
    }

    pub fn contains(&self, index: u8) -> bool {
        self.0 & (1 << index) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn pop_lsb(&mut self) -> Option<u8> {
        if self.is_empty() {
            return None;
        }

        let lsb = self.0.trailing_zeros() as u8;

        // bit magic to remove least significant bit
        self.0 &= self.0 - 1;
        Some(lsb)
    }
}
impl BitAnd for BitBoard {
    type Output = BitBoard;

    fn bitand(self, rhs: Self) -> Self::Output {
        BitBoard(self.0 & rhs.0)
    }
}

impl BitOr for BitBoard {
    type Output = BitBoard;

    fn bitor(self, rhs: Self) -> Self::Output {
        BitBoard(self.0 | rhs.0)
    }
}
impl Not for BitBoard {
    type Output = BitBoard;

    fn not(self) -> Self::Output {
        BitBoard(!self.0)
    }
}
impl Display for BitBoard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8).rev() {
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

#[derive(Clone, Copy)]
pub struct BitBoardCollection {
    // 6 pieces, 2 colours
    pub piece_boards: [[BitBoard; 6]; 2],
    pub mailbox: [Option<ChessPiece>; 64],
}

#[derive(Clone, Copy)]
pub struct PinInfo {
    pub pins: BitBoard,
    pub pin_rays: [BitBoard; 64],
}

#[derive(Clone, Copy)]
pub struct CheckInfo {
    pub in_check: bool,
    pub check_mask: BitBoard,
    pub num_checkers: u8,
}

use std::{
    fmt::{self, Display},
    ops::{BitAnd, BitOr, Not, RangeInclusive},
    sync::OnceLock,
};

use BitBoardCollection as BC;
use rand::{RngExt, SeedableRng, rngs::StdRng};

impl BitBoardCollection {
    pub fn new() -> Self {
        Self {
            piece_boards: [[BitBoard(0); 6]; 2],

            mailbox: [None; 64],
        }
    }

    pub fn attacks_by(&self, color: Color) -> BitBoard {
        let mut attacks = BitBoard(0);

        for kind in 0..6 {
            let piece = ChessPiece {
                kind: kind.try_into().unwrap(),
                color,
            };
            let mut bb = *self.get_board(&piece);

            while let Some(index) = bb.pop_lsb() {
                attacks.0 |= self.piece_attacks(index, &piece).0;
            }
        }

        attacks
    }

    pub fn is_in_check(&self, color: Color) -> bool {
        let king_bb = self
            .get_board(&ChessPiece {
                kind: PieceKind::King,
                color,
            })
            .0;

        let opponent = !color;
        let mut attacks = BitBoard(0);

        for kind in 0..6 {
            let piece = ChessPiece {
                kind: kind.try_into().unwrap(),
                color: opponent,
            };
            let mut bb = *self.get_board(&piece);
            while let Some(index) = bb.pop_lsb() {
                attacks.0 |= self.piece_attacks(index, &piece).0;
            }
        }

        king_bb & attacks.0 != 0
    }

    pub fn pin_info(&self, color: Color) -> PinInfo {
        let king = self
            .get_board(&ChessPiece {
                kind: PieceKind::King,
                color,
            })
            .0
            .trailing_zeros();
        let mut pins = BitBoard(0);
        let mut pin_rays = [BitBoard(!0); 64];

        let diagonals = BitBoard(
            self.get_board(&ChessPiece {
                kind: PieceKind::Bishop,
                color: !color,
            })
            .0 | self
                .get_board(&ChessPiece {
                    kind: PieceKind::Queen,
                    color: !color,
                })
                .0,
        );

        let orthogonals = BitBoard(
            self.get_board(&ChessPiece {
                kind: PieceKind::Rook,
                color: !color,
            })
            .0 | self
                .get_board(&ChessPiece {
                    kind: PieceKind::Queen,
                    color: !color,
                })
                .0,
        );

        for (directions, attackers) in [
            (&ROOK_DIRECTIONS[..], orthogonals),
            (&BISHOP_DIRECTIONS[..], diagonals),
        ] {
            for &(df, dr) in directions {
                let (mut f, mut r) = BC::decode_tile(king as u8);
                let (mut f, mut r) = (f as i8, r as i8);

                let mut ray = BitBoard(0);
                let mut potential_pin: Option<u8> = None;

                loop {
                    f += df;
                    r += dr;

                    if !(0..8).contains(&f) || !(0..8).contains(&r) {
                        // Out of bounds
                        break;
                    }

                    let sq = BC::encode_tile(f as u8, r as u8);
                    ray.insert(sq);

                    if let Some(piece) = self.piece_at_index(sq) {
                        if piece.color == color {
                            if potential_pin.is_none() {
                                // Ray hits our piece first, could be pin
                                potential_pin = Some(sq);
                            } else {
                                // Already hit our piece, cant be a pin
                                break;
                            }
                        } else {
                            // Either we already hit our piece, or this isnt a pin.
                            if let Some(pinned_sq) = potential_pin {
                                if attackers.contains(sq) {
                                    pins.insert(pinned_sq);
                                    pin_rays[pinned_sq as usize] = ray;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }

        PinInfo { pins, pin_rays }
    }

    pub fn check_info(&self, color: Color) -> CheckInfo {
        let mut check_mask = BitBoard(!0);
        let mut num_checkers: u8 = 0;

        let king_sq = self
            .get_board(&ChessPiece {
                kind: PieceKind::King,
                color,
            })
            .0
            .trailing_zeros() as u8;

        let pawn_checkers = BitBoard(
            self.pawn_attacks(king_sq, color).0 & self.get_board(&(!color).kind(PieceKind::Pawn)).0,
        );

        if !pawn_checkers.is_empty() {
            num_checkers += pawn_checkers.0.count_ones() as u8;
            // Kill the pawn
            check_mask = pawn_checkers
        }

        let knight_checkers = BitBoard(
            self.knight_attacks(king_sq).0 & self.get_board(&(!color).kind(PieceKind::Knight)).0,
        );

        if !knight_checkers.is_empty() {
            num_checkers += knight_checkers.0.count_ones() as u8;
            // Kill the knight
            check_mask = knight_checkers;
        }

        let dia_checkers = BitBoard(
            self.bishop_attacks(king_sq).0
                & (self.get_board(&(!color).kind(PieceKind::Bishop)).0
                    | self.get_board(&(!color).kind(PieceKind::Queen)).0),
        );

        if !dia_checkers.is_empty() {
            num_checkers += dia_checkers.0.count_ones() as u8;
            let checker_sq = dia_checkers.0.trailing_zeros() as u8;
            check_mask = self.ray_between(king_sq, checker_sq);
            check_mask.insert(checker_sq);
        }

        let ortho_checkers = BitBoard(
            self.rook_attacks(king_sq).0
                & (self.get_board(&(!color).kind(PieceKind::Rook)).0
                    | self.get_board(&(!color).kind(PieceKind::Queen)).0),
        );

        if !ortho_checkers.is_empty() {
            num_checkers += ortho_checkers.0.count_ones() as u8;
            let checker_sq = ortho_checkers.0.trailing_zeros() as u8;
            check_mask = self.ray_between(king_sq, checker_sq);
            check_mask.insert(checker_sq);
        }

        CheckInfo {
            in_check: num_checkers > 0,
            check_mask: if num_checkers == 0 {
                BitBoard(!0u64)
            } else if num_checkers == 1 {
                check_mask
            } else {
                BitBoard(0)
            },
            num_checkers,
        }
    }

    pub fn ray_between(&self, from: u8, to: u8) -> BitBoard {
        let (ff, fr) = BC::decode_tile(from);
        let (tf, tr) = BC::decode_tile(to);
        let df = (tf as i8 - ff as i8).signum();
        let dr = (tr as i8 - fr as i8).signum();
        let mut ray = BitBoard(0);
        let (mut f, mut r) = (ff as i8 + df, fr as i8 + dr);
        while (f as u8, r as u8) != (tf, tr) {
            ray.insert(BC::encode_tile(f as u8, r as u8));
            f += df;
            r += dr;
        }
        ray
    }

    pub fn piece_attacks(&self, index: u8, piece: &ChessPiece) -> BitBoard {
        match piece.kind {
            PieceKind::Pawn => self.pawn_attacks(index, piece.color),
            PieceKind::Knight => self.knight_attacks(index),
            PieceKind::Bishop => self.bishop_attacks(index),
            PieceKind::Rook => self.rook_attacks(index),
            PieceKind::Queen => self.bishop_attacks(index) | self.rook_attacks(index),
            PieceKind::King => self.king_attacks(index),
        }
    }

    pub fn bishop_attacks_occ(&self, index: u8, occupancy: BitBoard) -> BitBoard {
        Self::sliding_attacks(index, occupancy, &BISHOP_DIRECTIONS)
    }

    pub fn rook_attacks_occ(&self, index: u8, occupancy: BitBoard) -> BitBoard {
        Self::sliding_attacks(index, occupancy, &ROOK_DIRECTIONS)
    }

    pub fn bishop_attacks(&self, index: u8) -> BitBoard {
        Self::sliding_attacks(index, self.occupied(), &BISHOP_DIRECTIONS)
    }

    pub fn rook_attacks(&self, index: u8) -> BitBoard {
        Self::sliding_attacks(index, self.occupied(), &ROOK_DIRECTIONS)
    }

    pub fn pawn_attacks(&self, index: u8, color: Color) -> BitBoard {
        let forward = color.pawn_forward();
        let (file, _) = BC::decode_tile(index);
        let mut attacks = BitBoard(0);

        let left = index as i16 - 1 + forward;
        let right = index as i16 + 1 + forward;

        attacks.0 |= 1u64 << left;
        if file > 0 && left >= 0 && left < 64 {}
        if file < 7 && right >= 0 && right < 64 {
            attacks.0 |= 1u64 << right;
        }

        attacks
    }

    pub fn knight_attacks(&self, index: u8) -> BitBoard {
        let (file, rank) = BC::decode_tile(index);
        let mut attack = BitBoard(0);
        for (f, r) in KNIGHT_DIRECTIONS {
            let nf = file as i8 + f;
            let nr = rank as i8 + r;
            if (0..8).contains(&nf) && (0..8).contains(&nr) {
                attack.0 |= 1 << BC::encode_tile(nf as u8, nr as u8);
            }
        }
        attack
    }

    pub fn king_attacks(&self, index: u8) -> BitBoard {
        let (file, rank) = BC::decode_tile(index);
        let mut attack = BitBoard(0);
        for (f, r) in KING_DIRECTIONS {
            let nf = file as i8 + f;
            let nr = rank as i8 + r;
            if (0..8).contains(&nf) && (0..8).contains(&nr) {
                attack.0 |= 1 << BC::encode_tile(nf as u8, nr as u8);
            }
        }
        attack
    }

    pub fn sliding_attacks(index: u8, occupancy: BitBoard, directions: &[(i8, i8)]) -> BitBoard {
        let (file, rank) = BC::decode_tile(index);
        let mut attack = BitBoard(0);
        for &(f, r) in directions {
            let (mut nf, mut nr) = (file as i8, rank as i8);
            loop {
                nf += f;
                nr += r;
                if !(0..8).contains(&nf) || !(0..8).contains(&nr) {
                    break;
                }
                let tile = BC::encode_tile(nf as u8, nr as u8);
                attack.0 |= 1 << tile;
                if occupancy.contains(tile) {
                    break;
                }
            }
        }
        attack
    }

    pub fn decode_notation(not: &str) -> u8 {
        if not.len() != 2 {
            return 0;
        }
        let file_c = not.chars().nth(0).unwrap();
        let rank_c = not.chars().nth(1).unwrap();

        let file = FILES.iter().position(|&c| c == file_c).unwrap() as u8;
        let rank = rank_c.to_digit(10).unwrap() as u8 - 1;

        BC::encode_tile(file, rank)
    }

    pub fn encode_notation(index: u8) -> String {
        let (file, rank) = BC::decode_tile(index);
        [
            FILES[file as usize],
            char::from_digit(rank as u32 + 1, 10).unwrap(),
        ]
        .iter()
        .collect()
    }

    pub fn get_board(&self, piece: &ChessPiece) -> &BitBoard {
        &self.piece_boards[piece.color as usize][piece.kind as usize]
    }
    pub fn get_board_mut(&mut self, piece: &ChessPiece) -> &mut BitBoard {
        &mut self.piece_boards[piece.color as usize][piece.kind as usize]
    }

    pub fn insert(&mut self, index: u8, piece: &ChessPiece) {
        self.mailbox[index as usize] = Some(*piece);
        self.get_board_mut(piece).insert(index);
    }

    pub fn remove(&mut self, index: u8, piece: &ChessPiece) {
        self.mailbox[index as usize] = None;
        self.get_board_mut(piece).remove(index);
    }

    pub fn contains(&self, index: u8, piece: &ChessPiece) -> bool {
        self.get_board(piece).contains(index)
    }

    pub fn occupied_color(&self, color: Color) -> BitBoard {
        self.piece_boards[color as usize]
            .iter()
            .fold(BitBoard(0), |acc, b| BitBoard(acc.0 | b.0))
    }

    // All occupied squares
    pub fn occupied(&self) -> BitBoard {
        BitBoard(self.occupied_color(Color::White).0 | self.occupied_color(Color::Black).0)
    }

    pub fn piece_at_index(&self, index: u8) -> Option<ChessPiece> {
        return self.mailbox[index as usize];
    }

    pub fn from_fen(fen: &str) -> Self {
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

    pub fn encode_tile(file: u8, rank: u8) -> u8 {
        (rank * 8 + file)
    }

    pub fn decode_tile(index: u8) -> (u8, u8) {
        (index as u8 % 8, index as u8 / 8)
    }
}

impl Display for BitBoardCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8).rev() {
            writeln!(f, "+---+---+---+---+---+---+---+---+")?;
            write!(f, "|")?;
            for file in 0..8 {
                if let Some(p) = self.piece_at_index(BitBoardCollection::encode_tile(file, rank)) {
                    write!(f, " {} |", p.encode_fen())?;
                } else {
                    // empty
                    write!(f, "   |")?;
                }
            }
            write!(f, " {}", rank + 1)?;
            writeln!(f)?;
        }
        writeln!(f, "+---+---+---+---+---+---+---+---+")?;
        writeln!(f, "  a   b   c   d   e   f   g   h  ")?;

        Ok(())
    }
}

// -- Moves --

#[derive(Debug, Clone, Copy)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub flags: MoveFlags,
}
impl Move {
    pub fn new(from: u8, to: u8, flags: MoveFlags) -> Self {
        Self { from, to, flags }
    }

    pub fn modified(&self, to: u8, flags: MoveFlags) -> Self {
        Self {
            from: self.from,
            to,
            flags,
        }
    }
}
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct MoveFlags: u16 {
        const QUIET            = 0;
        const CAPTURE          = 1 << 0;
        const DOUBLE_PAWN_PUSH = 1 << 1;
        const EN_PASSANT       = 1 << 2;
        const CASTLE_KING      = 1 << 3;
        const CASTLE_QUEEN     = 1 << 4;

        const PROMOTE_N        = 1 << 5;
        const PROMOTE_B        = 1 << 6;
        const PROMOTE_R        = 1 << 7;
        const PROMOTE_Q        = 1 << 8;

        const IS_WHITE         = 1 << 9;
    }
}

static KNIGHT_DIRECTIONS: [(i8, i8); 8] = [
    (2, 1),
    (2, -1),
    (1, 2),
    (-1, 2),
    (-2, 1),
    (-2, -1),
    (1, -2),
    (-1, -2),
];

pub struct UndoMove {
    pub mv: Move,
    pub piece_captured: Option<ChessPiece>,
    pub rights: u8, // bitmask
    pub prev_fifty_move_counter: u32,
    pub en_passant_square: Option<u8>,
    pub hash: u64,
}

impl UndoMove {
    pub fn new(m: &Move, game: &Game) -> Self {
        let color = if game.white_turn {
            Color::White
        } else {
            Color::Black
        };
        let piece_captured = match m.flags.contains(MoveFlags::CAPTURE) {
            true => {
                if !m.flags.contains(MoveFlags::EN_PASSANT) {
                    game.board_collection.piece_at_index(m.to)
                } else {
                    game.board_collection
                        .piece_at_index((m.to as i16 - color.pawn_forward()) as u8)
                }
            }
            false => None,
        };

        let mut rights = 0u8;
        if game.k_white {
            rights |= 1;
        }
        if game.q_white {
            rights |= 2;
        }
        if game.k_black {
            rights |= 4;
        }
        if game.q_black {
            rights |= 8;
        }

        Self {
            mv: *m,
            piece_captured,
            rights,
            prev_fifty_move_counter: game.fifty_move_rule,
            en_passant_square: game.en_passant_square,
            hash: game.hash,
        }
    }
}

const FILES: [char; 8] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];

// Basically just FEN
#[derive(Clone, Copy)]
pub struct Game {
    pub board_collection: BitBoardCollection,
    pub white_turn: bool,
    pub en_passant_square: Option<u8>,
    pub q_white: bool,
    pub k_white: bool,
    pub q_black: bool,
    pub k_black: bool,
    pub fifty_move_rule: u32,
    pub move_count: u16,
    pub hash: u64,
}

impl Game {
    pub fn from_fen(fen: &str) -> Self {
        let fen_string = String::from(fen);

        let parts = fen_string.split_whitespace().collect::<Vec<&str>>();
        let white_turn = parts[1].chars().nth(0).unwrap() == 'w';

        let [k_white, q_white, k_black, q_black] =
            ['K', 'Q', 'k', 'q'].map(|c| parts[2].contains(c));
        let en_passant_square = if parts[3] == "-" {
            None
        } else {
            Some(BC::decode_notation(parts[3]))
        };
        let fifty_move_rule = parts[4].parse::<u32>().unwrap_or(0);
        let game_move = parts[5].parse::<u16>().unwrap_or(0);

        let mut game = Self {
            board_collection: BitBoardCollection::from_fen(
                // "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                // "r2qk2r/2p2ppp/p1n1bn2/1p1pp3/3P4/2N1PN2/PPP1BPPP/R2QK2R w KQkq - 0 9",
                fen,
            ),
            white_turn,
            en_passant_square,
            k_white,
            q_white,
            k_black,
            q_black,
            fifty_move_rule,
            move_count: game_move,
            hash: 0,
        };

        game.hash = zobrist().hash(&game);

        game
    }

    pub fn into_fen(&self) -> String {
        let mut board = String::new();

        for rank in (0..8).rev() {
            let mut empty_spaces = 0;
            for file in 0..8 {
                // println!("{}", empty_spaces);
                if let Some(piece) = self
                    .board_collection
                    .piece_at_index(BC::encode_tile(file, rank))
                {
                    if empty_spaces > 0 {
                        board.push(char::from_digit(empty_spaces, 10).unwrap());
                        empty_spaces = 0;
                    }

                    board.push(piece.encode_fen());
                } else {
                    empty_spaces += 1;
                }
            }
            if empty_spaces > 0 {
                board.push(char::from_digit(empty_spaces, 10).unwrap());
            }

            board.push('/');
        }

        board = board.trim_matches('/').to_string();
        let turn = if self.white_turn { "w" } else { "b" };
        let castling_rights =
            if (self.k_black || self.k_white || self.q_black || self.q_white) == false {
                String::from("-")
            } else {
                let rights = ['K', 'Q', 'k', 'q'];
                [self.k_white, self.q_white, self.k_black, self.q_black]
                    .iter()
                    .enumerate()
                    .map(|(i, r)| if *r { Some(rights[i]) } else { None })
                    .filter(|e| e.is_some())
                    .flatten()
                    .collect::<String>()
            };

        let ep = if let Some(square) = self.en_passant_square {
            BC::encode_notation(square)
        } else {
            String::from("-")
        };

        let half_move = self.fifty_move_rule.to_string();
        let move_count = self.move_count.to_string();

        [
            board,
            turn.to_string(),
            castling_rights,
            ep,
            half_move,
            move_count,
        ]
        .join(" ")
    }

    pub fn make_move(&mut self, m: &Move) -> UndoMove {
        let mut undo_move = UndoMove::new(m, self);

        let piece_from = self.board_collection.piece_at_index(m.from).unwrap();

        if piece_from.kind == PieceKind::Pawn || m.flags.contains(MoveFlags::CAPTURE) {
            self.fifty_move_rule = 0;
        } else {
            self.fifty_move_rule += 1;
        }

        let color = if self.white_turn {
            Color::White
        } else {
            Color::Black
        };

        // Clear en_passant square.
        if let Some(ep) = self.en_passant_square {
            let (file, _) = BC::decode_tile(ep);
            self.hash ^= zobrist().en_passant[file as usize];
        }
        self.en_passant_square = None;

        let is_promotion = m.flags.intersects(
            MoveFlags::PROMOTE_Q
                | MoveFlags::PROMOTE_R
                | MoveFlags::PROMOTE_N
                | MoveFlags::PROMOTE_B,
        );

        if m.flags.contains(MoveFlags::CAPTURE) {
            // En passant is a capture where the "capture space" is empty!
            if let Some(piece_captured) = self.board_collection.piece_at_index(m.to) {
                undo_move.piece_captured = Some(piece_captured);
                self.board_collection.remove(m.to, &piece_captured);
                self.hash ^= zobrist().pieces[piece_captured.color as usize]
                    [piece_captured.kind as usize][m.to as usize]
            }
        }

        self.board_collection.remove(m.from, &piece_from);
        self.hash ^=
            zobrist().pieces[piece_from.color as usize][piece_from.kind as usize][m.from as usize];

        if !is_promotion {
            self.board_collection.insert(m.to, &piece_from);
            self.hash ^= zobrist().pieces[piece_from.color as usize][piece_from.kind as usize]
                [m.to as usize];
        }

        if is_promotion {
            // Is promotion
            let new_kind = if m.flags.contains(MoveFlags::PROMOTE_Q) {
                PieceKind::Queen
            } else if m.flags.contains(MoveFlags::PROMOTE_R) {
                PieceKind::Rook
            } else if m.flags.contains(MoveFlags::PROMOTE_N) {
                PieceKind::Knight
            } else {
                PieceKind::Bishop
            };

            let promo_piece = color.kind(new_kind);

            self.board_collection.insert(m.to, &promo_piece);
            self.hash ^= zobrist().pieces[color as usize][new_kind as usize][m.to as usize];
        }

        if m.flags.contains(MoveFlags::EN_PASSANT) {
            let target = m.to as i16 - color.pawn_forward();
            let captured_pawn = ChessPiece {
                kind: PieceKind::Pawn,
                color: !color,
            };

            undo_move.piece_captured = Some(captured_pawn);
            self.board_collection.remove(target as u8, &captured_pawn);
            self.hash ^=
                zobrist().pieces[!color as usize][PieceKind::Pawn as usize][target as usize];
        }

        if m.flags.contains(MoveFlags::DOUBLE_PAWN_PUSH) {
            let ep_index = (m.from as i16 + color.pawn_forward()) as u8;
            self.en_passant_square = Some(ep_index);

            let (file, _) = BC::decode_tile(ep_index);

            self.hash ^= zobrist().en_passant[file as usize];
        }

        if m.flags.contains(MoveFlags::CASTLE_KING) {
            let rook_index: u8 = if self.white_turn { 7 } else { 63 };
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };
            self.board_collection.remove(rook_index, &rook);
            self.hash ^=
                zobrist().pieces[color as usize][PieceKind::Rook as usize][rook_index as usize];
            self.board_collection.insert(m.from + 1, &rook);
            self.hash ^=
                zobrist().pieces[color as usize][PieceKind::Rook as usize][(m.from + 1) as usize];
        }

        if m.flags.contains(MoveFlags::CASTLE_QUEEN) {
            let rook_index: u8 = if self.white_turn { 0 } else { 56 };
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };
            self.board_collection.remove(rook_index, &rook);
            self.hash ^=
                zobrist().pieces[color as usize][PieceKind::Rook as usize][rook_index as usize];
            self.board_collection.insert(m.from - 1, &rook);
            self.hash ^=
                zobrist().pieces[color as usize][PieceKind::Rook as usize][(m.from - 1) as usize];
        }

        if piece_from.kind == PieceKind::King {
            if self.white_turn {
                if self.k_white {
                    self.hash ^= zobrist().castling[0];
                    self.k_white = false;
                }
                if self.q_white {
                    self.hash ^= zobrist().castling[1];
                    self.q_white = false;
                }
            } else {
                if self.k_black {
                    self.hash ^= zobrist().castling[2];
                    self.k_black = false;
                }
                if self.q_black {
                    self.hash ^= zobrist().castling[3];
                    self.q_black = false;
                }
            }
        }
        if (m.from == 0 || m.to == 0) && self.q_white {
            self.q_white = false;
            self.hash ^= zobrist().castling[1];
        }

        if (m.from == 7 || m.to == 7) && self.k_white {
            self.k_white = false;
            self.hash ^= zobrist().castling[0];
        }

        if (m.from == 56 || m.to == 56) && self.q_black {
            self.q_black = false;
            self.hash ^= zobrist().castling[3];
        }

        if (m.from == 63 || m.to == 63) && self.k_black {
            self.k_black = false;
            self.hash ^= zobrist().castling[2];
        }

        self.white_turn = !self.white_turn;
        self.hash ^= zobrist().black_to_move;

        undo_move
    }

    pub fn undo_move(&mut self, u: &UndoMove) {
        self.white_turn = !self.white_turn;
        let color = if self.white_turn {
            Color::White
        } else {
            Color::Black
        };
        self.fifty_move_rule = u.prev_fifty_move_counter;
        self.en_passant_square = u.en_passant_square;

        let is_promotion = u.mv.flags.intersects(
            MoveFlags::PROMOTE_Q
                | MoveFlags::PROMOTE_R
                | MoveFlags::PROMOTE_N
                | MoveFlags::PROMOTE_B,
        );

        let piece_from = self.board_collection.piece_at_index(u.mv.to).unwrap();
        self.board_collection.remove(u.mv.to, &piece_from);

        if is_promotion {
            self.board_collection.insert(
                u.mv.from,
                &ChessPiece {
                    kind: PieceKind::Pawn,
                    color,
                },
            );
        } else {
            self.board_collection.insert(u.mv.from, &piece_from);
        }

        if !u.mv.flags.contains(MoveFlags::EN_PASSANT) && u.mv.flags.contains(MoveFlags::CAPTURE) {
            self.board_collection
                .insert(u.mv.to, &u.piece_captured.unwrap());
        } else if u.mv.flags.contains(MoveFlags::CAPTURE) {
            self.board_collection.insert(
                (u.mv.to as i16 - color.pawn_forward()) as u8,
                &ChessPiece {
                    kind: PieceKind::Pawn,
                    color: !color,
                },
            );
        }

        let prev = u.rights;
        self.k_white = prev & 1 != 0;
        self.q_white = prev & 2 != 0;
        self.k_black = prev & 4 != 0;
        self.q_black = prev & 8 != 0;

        if u.mv.flags.contains(MoveFlags::CASTLE_KING) {
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };
            self.board_collection.remove(u.mv.to - 1, &rook);
            self.board_collection.insert(u.mv.to + 1, &rook);
        }

        if u.mv.flags.contains(MoveFlags::CASTLE_QUEEN) {
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };
            self.board_collection.remove(u.mv.to + 1, &rook);
            self.board_collection.insert(u.mv.to - 2, &rook);
        }

        self.hash = u.hash;
    }
}

pub struct ZobristTable {
    pub pieces: [[[u64; 64]; 6]; 2],
    pub black_to_move: u64,
    pub castling: [u64; 4],
    pub en_passant: [u64; 8],
}

impl ZobristTable {
    pub fn new() -> Self {
        let mut rng = StdRng::seed_from_u64(67);

        let mut pieces = [[[0; 64]; 6]; 2];
        for color in 0..2 {
            for kind in 0..6 {
                for sq in 0..64 {
                    pieces[color][kind][sq] = rng.random();
                }
            }
        }

        let mut castling = [0u64; 4];
        for c in castling.iter_mut() {
            *c = rng.random();
        }

        let mut en_passant = [0; 8];
        for ep in en_passant.iter_mut() {
            *ep = rng.random()
        }

        Self {
            pieces,
            black_to_move: rng.random(),
            castling,
            en_passant,
        }
    }

    pub fn hash(&self, game: &Game) -> u64 {
        let mut h = 0;
        for sq in 0..64 {
            if let Some(piece) = game.board_collection.piece_at_index(sq) {
                h ^= self.pieces[piece.color as usize][piece.kind as usize][sq as usize]
            }
        }

        if !game.white_turn {
            h ^= self.black_to_move
        }
        if game.k_white {
            h ^= self.castling[0];
        }
        if game.q_white {
            h ^= self.castling[1];
        }
        if game.k_black {
            h ^= self.castling[2];
        }
        if game.q_black {
            h ^= self.castling[3];
        }

        if let Some(ep) = game.en_passant_square {
            let (file, _) = BC::decode_tile(ep);
            h ^= self.en_passant[file as usize];
        }

        h
    }
}

static ZOBRIST: OnceLock<ZobristTable> = OnceLock::new();

pub fn zobrist() -> &'static ZobristTable {
    ZOBRIST.get_or_init(|| ZobristTable::new())
}

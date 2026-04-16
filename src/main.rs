use std::io::Write;
use std::{
    collections::{HashMap, HashSet},
    f32::MIN,
    fmt::{self, Display},
    fs::OpenOptions,
    i32, io, mem,
    ops::{BitAnd, BitOr, Index, Not, RangeInclusive},
    sync::OnceLock,
    time::Instant,
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

impl PieceKind {
    fn value(&self) -> i32 {
        MG_PIECE_VALUES[*self as usize]
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Color {
    White,
    Black,
}

impl Color {
    fn pawn_forward(&self) -> i16 {
        match self {
            Color::White => 8,
            Color::Black => -8,
        }
    }
    fn pawn_start_rank(&self) -> RangeInclusive<u8> {
        match self {
            Color::White => 8..=15,
            Color::Black => 48..=55,
        }
    }
    fn pawn_promo_rank(&self) -> RangeInclusive<u8> {
        match self {
            Color::White => 56..=63,
            Color::Black => 0..=7,
        }
    }
    fn kind(&self, kind: PieceKind) -> ChessPiece {
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
    fn insert(&mut self, index: u8) {
        self.0 |= 1 << index;
    }

    fn remove(&mut self, index: u8) {
        self.0 &= !(1 << index);
    }

    fn contains(&self, index: u8) -> bool {
        self.0 & (1 << index) != 0
    }

    fn is_empty(&self) -> bool {
        self.0 == 0
    }

    fn pop_lsb(&mut self) -> Option<u8> {
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
struct BitBoardCollection {
    // 6 pieces, 2 colours
    piece_boards: [[BitBoard; 6]; 2],
    mailbox: [Option<ChessPiece>; 64],
}

#[derive(Clone, Copy)]
struct PinInfo {
    pins: BitBoard,
    pin_rays: [BitBoard; 64],
}

#[derive(Clone, Copy)]
struct CheckInfo {
    in_check: bool,
    check_mask: BitBoard,
    num_checkers: u8,
}

use BitBoardCollection as BC;
use rand::{RngExt, SeedableRng, rngs::StdRng};

impl BitBoardCollection {
    fn new() -> Self {
        Self {
            piece_boards: [[BitBoard(0); 6]; 2],

            mailbox: [None; 64],
        }
    }

    fn attacks_by(&self, color: Color) -> BitBoard {
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

    fn is_in_check(&self, color: Color) -> bool {
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

    fn pin_info(&self, color: Color) -> PinInfo {
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

    fn check_info(&self, color: Color) -> CheckInfo {
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

    fn ray_between(&self, from: u8, to: u8) -> BitBoard {
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

    fn piece_attacks(&self, index: u8, piece: &ChessPiece) -> BitBoard {
        match piece.kind {
            PieceKind::Pawn => self.pawn_attacks(index, piece.color),
            PieceKind::Knight => self.knight_attacks(index),
            PieceKind::Bishop => self.bishop_attacks(index),
            PieceKind::Rook => self.rook_attacks(index),
            PieceKind::Queen => self.bishop_attacks(index) | self.rook_attacks(index),
            PieceKind::King => self.king_attacks(index),
        }
    }

    fn bishop_attacks_occ(&self, index: u8, occupancy: BitBoard) -> BitBoard {
        Self::sliding_attacks(index, occupancy, &BISHOP_DIRECTIONS)
    }

    fn rook_attacks_occ(&self, index: u8, occupancy: BitBoard) -> BitBoard {
        Self::sliding_attacks(index, occupancy, &ROOK_DIRECTIONS)
    }

    fn bishop_attacks(&self, index: u8) -> BitBoard {
        Self::sliding_attacks(index, self.occupied(), &BISHOP_DIRECTIONS)
    }

    fn rook_attacks(&self, index: u8) -> BitBoard {
        Self::sliding_attacks(index, self.occupied(), &ROOK_DIRECTIONS)
    }

    fn pawn_attacks(&self, index: u8, color: Color) -> BitBoard {
        let forward = color.pawn_forward();
        let (file, _) = BC::decode_tile(index);
        let mut attacks = BitBoard(0);
        if file > 0 {
            attacks.0 |= 1 << (index as i16 - 1 + forward);
        }
        if file < 7 {
            attacks.0 |= 1 << (index as i16 + 1 + forward);
        }
        attacks
    }

    fn knight_attacks(&self, index: u8) -> BitBoard {
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

    fn king_attacks(&self, index: u8) -> BitBoard {
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

    fn sliding_attacks(index: u8, occupancy: BitBoard, directions: &[(i8, i8)]) -> BitBoard {
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

    fn decode_notation(not: &str) -> u8 {
        if not.len() != 2 {
            return 0;
        }
        let file_c = not.chars().nth(0).unwrap();
        let rank_c = not.chars().nth(1).unwrap();

        let file = FILES.iter().position(|&c| c == file_c).unwrap() as u8;
        let rank = rank_c.to_digit(10).unwrap() as u8 - 1;

        BC::encode_tile(file, rank)
    }

    fn encode_notation(index: u8) -> String {
        let (file, rank) = BC::decode_tile(index);
        [
            FILES[file as usize],
            char::from_digit(rank as u32 + 1, 10).unwrap(),
        ]
        .iter()
        .collect()
    }

    fn get_board(&self, piece: &ChessPiece) -> &BitBoard {
        &self.piece_boards[piece.color as usize][piece.kind as usize]
    }
    fn get_board_mut(&mut self, piece: &ChessPiece) -> &mut BitBoard {
        &mut self.piece_boards[piece.color as usize][piece.kind as usize]
    }

    fn insert(&mut self, index: u8, piece: &ChessPiece) {
        self.mailbox[index as usize] = Some(*piece);
        self.get_board_mut(piece).insert(index);
    }

    fn remove(&mut self, index: u8, piece: &ChessPiece) {
        self.mailbox[index as usize] = None;
        self.get_board_mut(piece).remove(index);
    }

    fn contains(&self, index: u8, piece: &ChessPiece) -> bool {
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
        return self.mailbox[index as usize];
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
        (rank * 8 + file)
    }

    fn decode_tile(index: u8) -> (u8, u8) {
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
struct Move {
    from: u8,
    to: u8,
    flags: MoveFlags,
}
impl Move {
    fn new(from: u8, to: u8, flags: MoveFlags) -> Self {
        Self { from, to, flags }
    }

    fn modified(&self, to: u8, flags: MoveFlags) -> Self {
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

// Castling masks, king side and queen side, black and white
const WHITE_K_EMPTY: u64 = (1 << 5) | (1 << 6);
const WHITE_K_SAFE: u64 = (1 << 4) | (1 << 5) | (1 << 6);
const WHITE_Q_EMPTY: u64 = (1 << 1) | (1 << 2) | (1 << 3);
const WHITE_Q_SAFE: u64 = (1 << 2) | (1 << 3) | (1 << 4);

const BLACK_K_EMPTY: u64 = (1 << 61) | (1 << 62);
const BLACK_K_SAFE: u64 = (1 << 60) | (1 << 61) | (1 << 62);
const BLACK_Q_EMPTY: u64 = (1 << 57) | (1 << 58) | (1 << 59);
const BLACK_Q_SAFE: u64 = (1 << 58) | (1 << 59) | (1 << 60);

static KING_DIRECTIONS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

static ROOK_DIRECTIONS: [(i8, i8); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
static BISHOP_DIRECTIONS: [(i8, i8); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

struct MoveGen<'a> {
    game: &'a Game,
    bc: &'a BitBoardCollection,
    white_attacks: BitBoard,
    black_attacks: BitBoard,
    occupied: BitBoard,
    white_occ: BitBoard,
    black_occ: BitBoard,
}
// attack = threats/protections
// quiets = empty spaces that the piece can move to
// captures = attack & opposite_color
// protections = attack & same_color
// moves = quiets | captures
// lists need to be split up in quiets and captures, pawns have special double push for en_passant tracking
impl<'a> MoveGen<'a> {
    fn new(game: &'a Game) -> Self {
        let mut mg = Self {
            game,
            bc: &game.board_collection,
            white_attacks: BitBoard(0),
            black_attacks: BitBoard(0),
            white_occ: BitBoard(0),
            black_occ: BitBoard(0),
            occupied: BitBoard(0),
        };

        if game.white_turn {
            mg.black_attacks = mg.bc.attacks_by(Color::Black);
        } else {
            mg.white_attacks = mg.bc.attacks_by(Color::White);
        }

        mg.white_occ = game.board_collection.occupied_color(Color::White);
        mg.black_occ = game.board_collection.occupied_color(Color::Black);
        mg.occupied = mg.white_occ | mg.black_occ;

        mg
    }

    fn new_engine(engine: &'a Engine) -> Self {
        let game = &engine.game;
        let mut mg = Self {
            game,
            bc: &game.board_collection,
            white_attacks: BitBoard(0),
            black_attacks: BitBoard(0),
            white_occ: BitBoard(0),
            black_occ: BitBoard(0),
            occupied: BitBoard(0),
        };

        mg.white_attacks = engine.cached_attacks[0];
        mg.black_attacks = engine.cached_attacks[1];
        mg.white_occ = game.board_collection.occupied_color(Color::White);
        mg.black_occ = game.board_collection.occupied_color(Color::Black);
        mg.occupied = mg.white_occ | mg.black_occ;

        mg
    }

    fn filter_legal(pseudo_moves: Vec<Move>, game: &mut Game, color: Color) -> Vec<Move> {
        let check_info = game.board_collection.check_info(color);
        let pin_info = game.board_collection.pin_info(color);

        pseudo_moves
            .into_iter()
            .filter(|mv| {
                let is_king = game
                    .board_collection
                    .piece_at_index(mv.from)
                    .map(|p| p.kind == PieceKind::King)
                    .unwrap_or(false);
                let is_pinned = pin_info.pins.contains(mv.from);
                let is_en_passant = mv.flags.contains(MoveFlags::EN_PASSANT);

                if is_king || is_en_passant {
                    let undo = game.make_move(mv);
                    let legal = !game.board_collection.is_in_check(color);
                    game.undo_move(&undo);
                    legal
                } else if is_pinned {
                    if check_info.in_check {
                        false
                    } else {
                        pin_info.pin_rays[mv.from as usize].contains(mv.to)
                    }
                } else {
                    !check_info.in_check || check_info.check_mask.contains(mv.to)
                }
            })
            .collect()
    }

    fn occupied_color(&self, color: Color) -> BitBoard {
        match color {
            Color::White => self.white_occ,
            Color::Black => self.black_occ,
        }
    }

    fn attacks_by(&self, color: Color) -> BitBoard {
        match color {
            Color::White => self.white_attacks,
            Color::Black => self.black_attacks,
        }
    }

    fn pseudo_legal_moves(&self, color: Color) -> Vec<Move> {
        let mut moves = Vec::new();

        for kind in 0..6 {
            let piece = ChessPiece {
                kind: kind.try_into().unwrap(),
                color,
            };

            let mut bb = *self.bc.get_board(&piece);

            while let Some(index) = bb.pop_lsb() {
                moves.extend(self.piece_moves_list(index, piece));
            }
        }

        moves
    }

    fn piece_captures(&self, index: u8, piece: ChessPiece) -> BitBoard {
        let color = piece.color;
        match piece.kind {
            PieceKind::Pawn => self.pawn_captures(index, color),
            PieceKind::Knight => self.knight_captures(index, color),
            PieceKind::Bishop => self.bishop_captures(index, color),
            PieceKind::Rook => self.rook_captures(index, color),
            PieceKind::Queen => {
                self.bishop_captures(index, color) | self.rook_captures(index, color)
            }
            PieceKind::King => self.king_captures(index, color),
        }
    }

    fn piece_quiets(&self, index: u8, piece: ChessPiece) -> BitBoard {
        let color = piece.color;
        match piece.kind {
            PieceKind::Pawn => self.pawn_quiets(index, color),
            PieceKind::Knight => self.knight_quiets(index, color),
            PieceKind::Bishop => self.bishop_quiets(index, color),
            PieceKind::Rook => self.rook_quiets(index, color),
            PieceKind::Queen => self.bishop_quiets(index, color) | self.rook_quiets(index, color),
            PieceKind::King => self.king_quiets(index, color),
        }
    }

    fn piece_moves(&self, index: u8, piece: ChessPiece) -> BitBoard {
        let color = piece.color;
        match piece.kind {
            PieceKind::Pawn => self.pawn_moves(index, color),
            PieceKind::Knight => self.knight_moves(index, color),
            PieceKind::Bishop => self.bishop_moves(index, color),
            PieceKind::Rook => self.rook_moves(index, color),
            PieceKind::Queen => self.bishop_moves(index, color) | self.rook_moves(index, color),
            PieceKind::King => self.king_moves(index, color),
        }
    }

    fn piece_moves_list(&self, index: u8, piece: ChessPiece) -> Vec<Move> {
        let color = piece.color;
        match piece.kind {
            PieceKind::Pawn => self.pawn_moves_list(index, color),
            PieceKind::Knight => self.knight_moves_list(index, color),
            PieceKind::Bishop => self.bishop_moves_list(index, color),
            PieceKind::Rook => self.rook_moves_list(index, color),
            PieceKind::Queen => {
                let mut moves = self.bishop_moves_list(index, color);
                moves.extend(self.rook_moves_list(index, color));
                moves
            }
            PieceKind::King => self.king_moves_list(index, color),
        }
    }

    fn basic_moves_list(&self, index: u8, piece: ChessPiece) -> Vec<Move> {
        let mut moves = Vec::new();
        let base_move = Move::new(index, 0, MoveFlags::empty());

        let mut quiets = self.piece_quiets(index, piece);

        while let Some(to) = quiets.pop_lsb() {
            moves.push(base_move.modified(to, MoveFlags::QUIET));
        }

        let mut captures = self.piece_captures(index, piece);

        while let Some(to) = captures.pop_lsb() {
            moves.push(base_move.modified(to, MoveFlags::CAPTURE));
        }

        moves
    }

    // -- Pawn --
    fn pawn_en_passant(&self, index: u8, color: Color) -> BitBoard {
        // En passant is a capture, but also a threat.

        let en_passant = self
            .game
            .en_passant_square
            .map(|ep| BitBoard(1 << ep))
            .unwrap_or(BitBoard(0));

        en_passant
    }
    fn pawn_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.pawn_attacks(index, color)
            & (self.pawn_en_passant(index, color) | self.occupied_color(!color))
    }
    fn pawn_single(&self, index: u8, color: Color) -> BitBoard {
        let forward = color.pawn_forward();
        let single = BitBoard(0b1 << (index as i16 + forward)) & !self.occupied;

        single
    }
    fn pawn_quiets(&self, index: u8, color: Color) -> BitBoard {
        // Pawn push seperate because pawns move differently than when they capture, pushes are moves without captures.
        let start_rank_range = color.pawn_start_rank();
        let forward = color.pawn_forward();
        let single = self.pawn_single(index, color);

        let double = if start_rank_range.contains(&index) && !single.is_empty() {
            BitBoard(0b1 << (index as i16 + forward * 2)) & !self.occupied
        } else {
            BitBoard(0)
        };

        single | double
    }

    fn pawn_moves(&self, index: u8, color: Color) -> BitBoard {
        self.pawn_quiets(index, color) | self.pawn_captures(index, color)
    }

    fn pawn_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        let base_move = Move::new(index, 0, MoveFlags::empty());
        let mut moves: Vec<Move> = Vec::new();
        let promo_rank = color.pawn_promo_rank();

        let mut singles = self.pawn_single(index, color);

        while let Some(to) = singles.pop_lsb() {
            if !promo_rank.contains(&to) {
                moves.push(base_move.modified(to, MoveFlags::QUIET));
            } else {
                for t in [
                    MoveFlags::PROMOTE_Q,
                    MoveFlags::PROMOTE_R,
                    MoveFlags::PROMOTE_N,
                    MoveFlags::PROMOTE_B,
                ] {
                    moves.push(base_move.modified(to, MoveFlags::QUIET | t))
                }
            }
        }

        // Pawn quiets without single pushes are double pushes
        let mut doubles = self.pawn_quiets(index, color) & !self.pawn_single(index, color);

        while let Some(to) = doubles.pop_lsb() {
            moves.push(base_move.modified(to, MoveFlags::QUIET | MoveFlags::DOUBLE_PAWN_PUSH));
        }

        let mut captures = self.pawn_captures(index, color);

        while let Some(to) = captures.pop_lsb() {
            let is_en_passant = self.game.en_passant_square == Some(to);

            let ep = if is_en_passant {
                MoveFlags::EN_PASSANT
            } else {
                MoveFlags::empty()
            };

            if !promo_rank.contains(&to) {
                moves.push(base_move.modified(to, MoveFlags::CAPTURE | ep));
            } else {
                for t in [
                    MoveFlags::PROMOTE_Q,
                    MoveFlags::PROMOTE_R,
                    MoveFlags::PROMOTE_N,
                    MoveFlags::PROMOTE_B,
                ] {
                    moves.push(base_move.modified(to, MoveFlags::CAPTURE | t | ep))
                }
            }
        }

        moves
    }

    // -- Knight --

    fn knight_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.knight_attacks(index) & self.occupied_color(!color)
    }
    fn knight_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.knight_attacks(index) & !self.occupied
    }
    fn knight_moves(&self, index: u8, color: Color) -> BitBoard {
        self.knight_quiets(index, color) | self.knight_captures(index, color)
    }
    fn knight_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        self.basic_moves_list(
            index,
            ChessPiece {
                kind: PieceKind::Knight,
                color,
            },
        )
    }

    // -- King --
    fn king_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.king_attacks(index)
            & self.occupied_color(!color)
            & !(self.attacks_by(!color))
    }
    fn king_quiets(&self, index: u8, color: Color) -> BitBoard {
        // King cant move to an attacked square
        self.game.board_collection.king_attacks(index) & !(self.attacks_by(!color)) & !self.occupied
    }
    fn king_castling_moves(&self, index: u8, color: Color, king_side: bool) -> BitBoard {
        let is_white = color == Color::White;
        let comb_index = is_white as usize * 2 + king_side as usize;

        let rights = [
            self.game.q_black,
            self.game.k_black,
            self.game.q_white,
            self.game.k_white,
        ][comb_index];
        let empty = [BLACK_Q_EMPTY, BLACK_K_EMPTY, WHITE_Q_EMPTY, WHITE_K_EMPTY][comb_index];
        let safe = [BLACK_Q_SAFE, BLACK_K_SAFE, WHITE_Q_SAFE, WHITE_K_SAFE][comb_index];

        let attacks = self.attacks_by(!color);
        let occupied = self.occupied;
        let target = if king_side { index + 2 } else { index - 2 };

        if rights && (safe & attacks.0) == 0 && (empty & occupied.0) == 0 {
            BitBoard(1 << target)
        } else {
            BitBoard(0)
        }
    }

    fn king_moves(&self, index: u8, color: Color) -> BitBoard {
        self.king_quiets(index, color)
            | self.king_captures(index, color)
            | self.king_castling_moves(index, color, true)
            | self.king_castling_moves(index, color, false)
    }

    fn king_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        let mut moves = Vec::new();
        let base_move = Move::new(index, 0, MoveFlags::empty());

        let mut quiets = self.king_quiets(index, color);

        while let Some(to) = quiets.pop_lsb() {
            moves.push(base_move.modified(to, MoveFlags::QUIET));
        }

        let mut captures = self.king_captures(index, color);

        while let Some(to) = captures.pop_lsb() {
            moves.push(base_move.modified(to, MoveFlags::CAPTURE));
        }

        let mut castles_k = self.king_castling_moves(index, color, true);

        while let Some(to) = castles_k.pop_lsb() {
            moves.push(base_move.modified(to, MoveFlags::CASTLE_KING));
        }

        let mut castles_q = self.king_castling_moves(index, color, false);

        while let Some(to) = castles_q.pop_lsb() {
            moves.push(base_move.modified(to, MoveFlags::CASTLE_QUEEN));
        }

        moves
    }

    // -- Rook --

    fn rook_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.rook_attacks(index) & self.occupied_color(!color)
    }
    fn rook_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.rook_attacks(index) & !self.occupied
    }
    fn rook_moves(&self, index: u8, color: Color) -> BitBoard {
        self.rook_quiets(index, color) | self.rook_captures(index, color)
    }
    fn rook_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        self.basic_moves_list(
            index,
            ChessPiece {
                kind: PieceKind::Rook,
                color,
            },
        )
    }

    // -- Bishop --

    fn bishop_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.bishop_attacks(index) & self.occupied_color(!color)
    }
    fn bishop_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.bishop_attacks(index) & !self.occupied
    }
    fn bishop_moves(&self, index: u8, color: Color) -> BitBoard {
        self.bishop_quiets(index, color) | self.bishop_captures(index, color)
    }
    fn bishop_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        self.basic_moves_list(
            index,
            ChessPiece {
                kind: PieceKind::Bishop,
                color,
            },
        )
    }

    // Queen is bishop | rook
}

struct UndoMove {
    mv: Move,
    piece_captured: Option<ChessPiece>,
    rights: u8, // bitmask
    prev_fifty_move_counter: u32,
    en_passant_square: Option<u8>,
    hash: u64,
}

impl UndoMove {
    fn new(m: &Move, game: &Game) -> Self {
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
struct Game {
    board_collection: BitBoardCollection,
    white_turn: bool,
    en_passant_square: Option<u8>,
    q_white: bool,
    k_white: bool,
    q_black: bool,
    k_black: bool,
    fifty_move_rule: u32,
    move_count: u16,
    hash: u64,
}

impl Game {
    fn from_fen(fen: &str) -> Self {
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

    fn into_fen(&self) -> String {
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

    fn make_move(&mut self, m: &Move) -> UndoMove {
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

    fn undo_move(&mut self, u: &UndoMove) {
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

struct ZobristTable {
    pieces: [[[u64; 64]; 6]; 2],
    black_to_move: u64,
    castling: [u64; 4],
    en_passant: [u64; 8],
}

impl ZobristTable {
    fn new() -> Self {
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

    fn hash(&self, game: &Game) -> u64 {
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

fn zobrist() -> &'static ZobristTable {
    ZOBRIST.get_or_init(|| ZobristTable::new())
}

#[derive(Clone, Copy)]
enum TTFlag {
    Exact,
    LowerBound,
    UpperBound,
}

#[derive(Clone, Copy)]
struct TTEntry {
    hash: u64,
    depth: u8,
    score: i32,
    flag: TTFlag,
    best_move: Option<Move>,
}

struct Engine {
    game: Game,
    history: HashMap<u64, u8>,
    tt: Vec<Option<TTEntry>>,
    nodes: u64,
    game_history: HashMap<u64, u8>, // Order doesnt matter, can appear multiple times
    last_score: i32,
    stop: bool,
    cached_attacks: [BitBoard; 2],
    cache_hash: u64,
    killers: [[Option<Move>; 2]; 64],
    history_table: [[[i32; 64]; 64]; 2],
}

// Values from PeSTO

static MG_PAWN_TABLE: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 98, 134, 61, 95, 68, 126, 34, -11, -6, 7, 26, 31, 65, 56, 25, -20, -14,
    13, 6, 21, 23, 12, 17, -23, -27, -2, -5, 12, 17, 6, 10, -25, -26, -4, -4, -10, 3, 3, 33, -12,
    -35, -1, -20, -23, -15, 24, 38, -22, 0, 0, 0, 0, 0, 0, 0, 0,
];

static EG_PAWN_TABLE: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 178, 173, 158, 134, 147, 132, 165, 187, 94, 100, 85, 67, 56, 53, 82,
    84, 32, 24, 13, 5, -2, 4, 17, 17, 13, 9, -3, -7, -7, -8, 3, -1, 4, 7, -6, 1, 0, -5, -1, -8, 13,
    8, 8, 10, 13, 0, 2, -7, 0, 0, 0, 0, 0, 0, 0, 0,
];

static MG_KNIGHT_TABLE: [i32; 64] = [
    -167, -89, -34, -49, 61, -97, -15, -107, -73, -41, 72, 36, 23, 62, 7, -17, -47, 60, 37, 65, 84,
    129, 73, 44, -9, 17, 19, 53, 37, 69, 18, 22, -13, 4, 16, 13, 28, 19, 21, -8, -23, -9, 12, 10,
    19, 17, 25, -16, -29, -53, -12, -3, -1, 18, -14, -19, -105, -21, -58, -33, -17, -28, -19, -23,
];

static EG_KNIGHT_TABLE: [i32; 64] = [
    -58, -38, -13, -28, -31, -27, -63, -99, -25, -8, -25, -2, -9, -25, -24, -52, -24, -20, 10, 9,
    -1, -9, -19, -41, -17, 3, 22, 22, 22, 11, 8, -18, -18, -6, 16, 25, 16, 17, 4, -18, -23, -3, -1,
    15, 10, -3, -20, -22, -42, -20, -10, -5, -2, -20, -23, -44, -29, -51, -23, -15, -22, -18, -50,
    -64,
];

static MG_BISHOP_TABLE: [i32; 64] = [
    -29, 4, -82, -37, -25, -42, 7, -8, -26, 16, -18, -13, 30, 59, 18, -47, -16, 37, 43, 40, 35, 50,
    37, -2, -4, 5, 19, 50, 37, 37, 7, -2, -6, 13, 13, 26, 34, 12, 10, 4, 0, 15, 15, 15, 14, 27, 18,
    10, 4, 15, 16, 0, 7, 21, 33, 1, -33, -3, -14, -21, -13, -12, -39, -21,
];

static EG_BISHOP_TABLE: [i32; 64] = [
    -14, -21, -11, -8, -7, -9, -17, -24, -8, -4, 7, -12, -3, -13, -4, -14, 2, -8, 0, -1, -2, 6, 0,
    4, -3, 9, 12, 9, 14, 10, 3, 2, -6, 3, 13, 19, 7, 10, -3, -9, -12, -3, 8, 10, 13, 3, -7, -15,
    -14, -18, -7, -1, 4, -9, -15, -27, -23, -9, -23, -5, -9, -16, -5, -17,
];

static MG_ROOK_TABLE: [i32; 64] = [
    32, 42, 32, 51, 63, 9, 31, 43, 27, 32, 58, 62, 80, 67, 26, 44, -5, 19, 26, 36, 17, 45, 61, 16,
    -24, -11, 7, 26, 24, 35, -8, -20, -36, -26, -12, -1, 9, -7, 6, -23, -45, -25, -16, -17, 3, 0,
    -5, -33, -44, -16, -20, -9, -1, 11, -6, -71, -19, -13, 1, 17, 16, 7, -37, -26,
];

static EG_ROOK_TABLE: [i32; 64] = [
    13, 10, 18, 15, 12, 12, 8, 5, 11, 13, 13, 11, -3, 3, 8, 3, 7, 7, 7, 5, 4, -3, -5, -3, 4, 3, 13,
    1, 2, 1, -1, 2, 3, 5, 8, 4, -5, -6, -8, -11, -4, 0, -5, -1, -7, -12, -8, -16, -6, -6, 0, 2, -9,
    -9, -11, -3, -9, 2, 3, -1, -5, -13, 4, -20,
];

static MG_QUEEN_TABLE: [i32; 64] = [
    -28, 0, 29, 12, 59, 44, 43, 45, -24, -39, -5, 1, -16, 57, 28, 54, -13, -17, 7, 8, 29, 56, 47,
    57, -27, -27, -16, -16, -1, 17, -2, 1, -9, -26, -9, -10, -2, -4, 3, -3, -14, 2, -11, -2, -5, 2,
    14, 5, -35, -8, 11, 2, 8, 15, -3, 1, -1, -18, -9, 10, -15, -25, -31, -50,
];

static EG_QUEEN_TABLE: [i32; 64] = [
    -9, 22, 22, 27, 27, 19, 10, 20, -17, 20, 32, 41, 58, 25, 30, 0, -20, 6, 9, 49, 47, 35, 19, 9,
    3, 22, 24, 45, 57, 40, 57, 36, -18, 28, 19, 47, 31, 34, 39, 23, -16, -27, 15, 6, 9, 17, 10, 5,
    -22, -23, -30, -16, -16, -23, -36, -32, -33, -28, -22, -43, -5, -32, -20, -41,
];

static MG_KING_TABLE: [i32; 64] = [
    -65, 23, 16, -15, -56, -34, 2, 13, 29, -1, -20, -7, -8, -4, -38, -29, -9, 24, 2, -16, -20, 6,
    22, -22, -17, -20, -12, -27, -30, -25, -14, -36, -49, -1, -27, -39, -46, -44, -33, -51, -14,
    -14, -22, -46, -44, -30, -15, -27, 1, 7, -8, -64, -43, -16, 9, 8, -15, 36, 12, -54, 8, -28, 24,
    14,
];

static EG_KING_TABLE: [i32; 64] = [
    -74, -35, -18, -18, -11, 15, 4, -17, -12, 17, 14, 17, 17, 38, 23, 11, 10, 17, 23, 15, 20, 45,
    44, 13, -8, 22, 24, 27, 26, 33, 26, 3, -18, -4, 21, 24, 27, 23, 9, -11, -19, -3, 11, 21, 23,
    16, 7, -9, -27, -11, 4, 13, 14, 4, -5, -17, -53, -34, -21, -11, -28, -14, -24, -43,
];

static GAMEPHASE_INC: [i32; 6] = [0, 1, 1, 2, 4, 0]; // pawn, knight, bishop, rook, queen, king
static PASSED_PAWN_BONUS: [i32; 8] = [0, 0, 10, 20, 35, 55, 80, 120];

static CENTER_BONUS: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 1, 2, 2, 2, 2, 1, 0, 0, 1, 2, 3, 3, 2, 1, 0,
    0, 1, 2, 3, 3, 2, 1, 0, 0, 1, 2, 2, 2, 2, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

static CONTROL_WEIGHTS: [i32; 6] = [
    3, // pawn control is very valuable
    2, // knight
    2, // bishop
    1, // rook
    1, // queen
    0, // king
];

static ATTACK_WEIGHTS: [i32; 6] = [0, 2, 2, 3, 5, 0]; // pawn, knight, bishop, rook, queen, king

static SAFETY_TABLE: [i32; 100] = {
    let mut table = [0i32; 100];
    let mut i = 0;
    while i < 100 {
        table[i] = (i * i) as i32 / 10;
        i += 1;
    }
    table
};

static MG_PIECE_VALUES: [i32; 6] = [82, 337, 365, 477, 1025, 0]; // P N B R Q K
static EG_PIECE_VALUES: [i32; 6] = [94, 281, 297, 512, 936, 0];

impl Engine {
    fn new() -> Self {
        Engine {
            game: Game::from_fen(START_POS),
            history: HashMap::new(),
            game_history: HashMap::new(),
            tt: vec![None; 1 << 22],
            last_score: 0,
            stop: false,
            nodes: 0,
            cache_hash: 0,
            cached_attacks: [BitBoard(0); 2],
            killers: [[None; 2]; 64],
            history_table: [[[0; 64]; 64]; 2],
        }
    }

    fn smallest_attacker(
        &self,
        index: u8,
        color: Color,
        occupancy: BitBoard,
    ) -> Option<(u8, PieceKind)> {
        let bc = &self.game.board_collection;

        for k in 0..6usize {
            let kind = k.try_into().unwrap();
            let piece = &color.kind(kind);
            // ignore captured attackers
            let attackers = BitBoard(bc.get_board(piece).0 & occupancy.0);

            let possible_attacks = match kind {
                PieceKind::Pawn => bc.pawn_attacks(index, !color),
                PieceKind::Bishop => bc.bishop_attacks_occ(index, occupancy),
                PieceKind::Rook => bc.rook_attacks_occ(index, occupancy),
                PieceKind::Queen => {
                    bc.bishop_attacks_occ(index, occupancy) | bc.rook_attacks_occ(index, occupancy)
                }
                _ => bc.piece_attacks(index, piece),
            };

            let comb = attackers.0 & possible_attacks.0;

            if comb != 0 {
                return Some((comb.trailing_zeros() as u8, kind));
            }
        }

        None
    }

    fn see(&self, target_sq: u8, target_value: i32, from_sq: u8, attacker_value: i32) -> i32 {
        let mut gain = [0i32; 32];
        let mut d = 0;
        gain[d] = target_value;

        let mut occupancy = self.game.board_collection.occupied();
        let mut from = from_sq;
        let mut attacker_val = attacker_value;
        let mut side = if self.game.white_turn {
            Color::Black
        } else {
            Color::White
        };
        loop {
            // Remove the previous attacker
            occupancy.0 &= !(1 << from);

            let next = self.smallest_attacker(target_sq, side, occupancy);

            match next {
                Some((new_from, new_kind)) => {
                    d += 1;
                    gain[d] = attacker_val - gain[d - 1];

                    if gain[d].max(-gain[d - 1]) < 0 {
                        break;
                    }

                    from = new_from;
                    attacker_val = new_kind.value();
                    side = !side;
                }
                None => break,
            }
        }

        while d > 0 {
            gain[d - 1] = -(-gain[d - 1]).max(gain[d]);
            d -= 1;
        }

        gain[0]
    }

    fn refresh_cache(&mut self) {
        if self.cache_hash == self.game.hash {
            return; // Nothing to do
        }

        let bc = &self.game.board_collection;

        for c in 0..2usize {
            let color = Color::try_from(c).unwrap();
            let mut attacks = BitBoard(0);

            for kind in 0..6usize {
                let piece = color.kind(kind.try_into().unwrap());
                let mut bb = *bc.get_board(&piece);
                while let Some(idx) = bb.pop_lsb() {
                    let piece_attack = bc.piece_attacks(idx, &piece);
                    attacks.0 |= piece_attack.0;
                }
            }

            self.cached_attacks[c] = attacks;
        }

        self.cache_hash = self.game.hash;
    }

    fn hanging_penalty(
        &self,
        enemy_attacks: BitBoard,
        friendly_attacks: BitBoard,
        mut our_pieces: BitBoard,
    ) -> i32 {
        let bc = &self.game.board_collection;

        let mut penalty = 0;

        while let Some(sq) = our_pieces.pop_lsb() {
            let piece = bc.piece_at_index(sq).unwrap();
            if piece.kind == PieceKind::King {
                continue;
            }

            let piece_value = MG_PIECE_VALUES[piece.kind as usize];
            if enemy_attacks.contains(sq) {
                if !friendly_attacks.contains(sq) {
                    // Completely undefended
                    penalty += piece_value as i32 / 2;
                } else {
                    // Defended but check if attacked by cheaper piece
                    // Find cheapest attacker
                    for kind in 0..5usize {
                        let attacker = (!piece.color).kind(kind.try_into().unwrap());
                        if bc.get_board(&attacker).0 & bc.piece_attacks(sq, &attacker).0 != 0 {
                            let attacker_value = MG_PIECE_VALUES[attacker.kind as usize];
                            if attacker_value < piece_value {
                                penalty += (piece_value - attacker_value) as i32 / 2;
                            }
                            break; // only care about cheapest
                        }
                    }
                }
            }
        }

        penalty
    }

    fn king_safety(&self, king_sq: u8, friendly_pieces: BitBoard, enemy_attacks: BitBoard) -> i32 {
        let mut around = BitBoard(0);
        let (file, rank) = BC::decode_tile(king_sq);
        for (df, dr) in KING_DIRECTIONS {
            let (nf, nr) = (file as i8 + df, rank as i8 + dr);
            if (0..8).contains(&nf) && (0..8).contains(&nr) {
                around.0 |= 1 << BC::encode_tile(nf as u8, nr as u8);
            }
        }

        let safe_defenders = (around & friendly_pieces & !enemy_attacks).0.count_ones() as i32;
        let attacked_empty = (around & !friendly_pieces & enemy_attacks).0.count_ones() as i32;

        safe_defenders * 4 - attacked_empty * 8
    }

    fn static_eval(&self) -> i32 {
        let color = if self.game.white_turn {
            Color::White
        } else {
            Color::Black
        };
        let bc = &self.game.board_collection;
        let friendly_attacks = self.cached_attacks[color as usize];
        let enemy_attacks = self.cached_attacks[(!color) as usize];

        let friendly_pieces = bc.occupied_color(color);
        let enemy_pieces = bc.occupied_color(!color);

        let mut mg = [0; 2];
        let mut eg = [0; 2];
        let mut game_phase = 0i32;
        let mut white_king_sq = 0;
        let mut black_king_sq = 0;

        let mut friendly_bishops = 0;
        let mut enemy_bishops = 0;

        for sq in 0..64u8 {
            if let Some(piece) = self.game.board_collection.piece_at_index(sq) {
                let color_idx = piece.color as usize;

                let table_sq = if piece.color == Color::White {
                    sq as usize
                } else {
                    (sq ^ 56) as usize // mirror black
                };

                let (mg_val, eg_val) = match piece.kind {
                    PieceKind::Pawn => (MG_PAWN_TABLE[table_sq], EG_PAWN_TABLE[table_sq]),
                    PieceKind::Knight => (MG_KNIGHT_TABLE[table_sq], EG_KNIGHT_TABLE[table_sq]),
                    PieceKind::Bishop => (MG_BISHOP_TABLE[table_sq], EG_BISHOP_TABLE[table_sq]),
                    PieceKind::Rook => (MG_ROOK_TABLE[table_sq], EG_ROOK_TABLE[table_sq]),
                    PieceKind::Queen => (MG_QUEEN_TABLE[table_sq], EG_QUEEN_TABLE[table_sq]),
                    PieceKind::King => (MG_KING_TABLE[table_sq], EG_KING_TABLE[table_sq]),
                };

                if piece.kind == PieceKind::King {
                    if piece.color == Color::White {
                        white_king_sq = sq;
                    } else {
                        black_king_sq = sq;
                    }
                }

                if piece.kind == PieceKind::Bishop {
                    if piece.color == color {
                        friendly_bishops += 1;
                    } else {
                        enemy_bishops += 1;
                    }
                }

                let kind_idx = piece.kind as usize;
                mg[color_idx] += mg_val + MG_PIECE_VALUES[kind_idx];
                eg[color_idx] += eg_val + EG_PIECE_VALUES[kind_idx];

                game_phase += GAMEPHASE_INC[piece.kind as usize];
            }
        }

        let side = if self.game.white_turn { 0 } else { 1 };
        let other = 1 - side;

        let mg_score = mg[side] - mg[other];
        let eg_score = eg[side] - eg[other];
        let mg_phase = game_phase.min(24);
        let eg_phase = 24 - mg_phase;

        // let hanging = self.hanging_penalty(enemy_attacks, friendly_attacks, friendly_pieces)
        //     - self.hanging_penalty(friendly_attacks, enemy_attacks, enemy_pieces);
        let hanging = 0;

        let king_sq = if self.game.white_turn {
            white_king_sq
        } else {
            black_king_sq
        };
        let enemy_king_sq = if self.game.white_turn {
            black_king_sq
        } else {
            white_king_sq
        };

        let friendly_pawns = *self
            .game
            .board_collection
            .get_board(&color.kind(PieceKind::Pawn));

        let enemy_pawns = *self
            .game
            .board_collection
            .get_board(&(!color).kind(PieceKind::Pawn));

        let pawns = self.pawn_bonus(friendly_pawns, enemy_pawns, color)
            - self.pawn_bonus(enemy_pawns, friendly_pawns, !color);

        let bishops = self.bishop_bonus(friendly_bishops) - self.bishop_bonus(enemy_bishops);

        let king_safety = (self.king_safety(king_sq, friendly_pieces, enemy_attacks)
            - self.king_safety(enemy_king_sq, enemy_pieces, friendly_attacks))
            * mg_phase
            / 24;

        let castling_rights_bonus = {
            let our_rights = if self.game.white_turn {
                self.game.k_white as i32 + self.game.q_white as i32
            } else {
                self.game.k_black as i32 + self.game.q_black as i32
            };
            let their_rights = if self.game.white_turn {
                self.game.k_black as i32 + self.game.q_black as i32
            } else {
                self.game.k_white as i32 + self.game.q_white as i32
            };
            (our_rights - their_rights) * 20
        };

        (mg_score * mg_phase + eg_score * eg_phase) / 24 - hanging
            + king_safety
            + bishops
            + castling_rights_bonus
            + pawns
    }

    fn pawn_bonus(&self, friendly_pawns: BitBoard, enemy_pawns: BitBoard, color: Color) -> i32 {
        // Passed pawns detection
        let doubled_pawns_penalty = 20;
        let file_mask = 0x0101010101010101;
        let mut bonus = 0;

        let mut friendly_pawns_clone = friendly_pawns;
        while let Some(pawn_sq) = friendly_pawns_clone.pop_lsb() {
            let (file, rank) = BC::decode_tile(pawn_sq);

            let mut scan_mask = file_mask << file;

            if file > 0 {
                scan_mask |= file_mask << (file - 1);
            }
            if file < 7 {
                scan_mask |= file_mask << (file + 1);
            }

            if color == Color::White {
                scan_mask &= !0u64 << (rank * 8);
            } else {
                scan_mask &= (1u64 << (rank * 8)).wrapping_sub(1);
            }

            // println!("{} {:?}", BitBoard(scan_mask), color);
            if scan_mask & enemy_pawns.0 == 0 {
                let bonus_rank = if color == Color::White {
                    rank
                } else {
                    7 - rank
                };

                bonus += PASSED_PAWN_BONUS[bonus_rank as usize];
            }

            let adjacent_file_mask = {
                let mut m = file_mask << file;
                if file > 0 {
                    m |= file_mask << (file - 1);
                }
                if file < 7 {
                    m |= file_mask << (file + 1);
                }
                m & !(file_mask << file) // exclude own file
            };

            if adjacent_file_mask & friendly_pawns.0 == 0 {
                bonus -= 15;
            }
        }

        for file in 0..8 {
            let pawns_on_file = (file_mask << file & friendly_pawns.0).count_ones();
            if pawns_on_file > 1 {
                bonus -= doubled_pawns_penalty * (pawns_on_file - 1) as i32;
            }
        }

        bonus
    }

    fn bishop_bonus(&self, bishop_count: u8) -> i32 {
        if bishop_count < 2 { 0 } else { 30 }
    }

    fn search(&mut self, max_depth: u8, time_ms: u64) -> Option<Move> {
        self.killers = [[None; 2]; 64];
        self.history_table = [[[0; 64]; 64]; 2];

        self.tt.iter_mut().for_each(|e| *e = None);
        let start = Instant::now();
        let mut best_move = None;
        self.nodes = 0;
        self.stop = false;
        let deadline = time_ms;
        let mut score_history: Vec<i32> = Vec::new();

        for depth in 1..=max_depth {
            self.history.clear();
            let mut delta = if score_history.len() >= 4 {
                let recent_swing = (score_history[score_history.len() - 1]
                    - score_history[score_history.len() - 3])
                    .abs();
                50.max(recent_swing / 2).min(200)
            } else {
                50
            };

            let guess = if score_history.len() >= 2 {
                score_history[score_history.len() - 2]
            } else {
                self.last_score
            };

            let (mut alpha, mut beta) = if depth == 1 {
                (-999_999_999, 999_999_999)
            } else {
                (guess - delta, guess + delta)
            };

            loop {
                match self.search_at_depth(depth, alpha, beta, &start, deadline) {
                    None => break, // timeout or no legal moves
                    Some((mv, score)) => {
                        if score <= alpha {
                            // fail-low: widen alpha, keep beta
                            delta *= 2;
                            alpha = guess - delta;
                        } else if score >= beta {
                            // fail-high: widen beta, keep alpha, but still record the move
                            best_move = Some(mv);
                            self.last_score = score;
                            if score >= 900_000 {
                                beta = 999_999_999;
                            } else {
                                delta *= 2;
                                beta = guess + delta;
                            }
                        } else {
                            // score is inside window — done
                            best_move = Some(mv);
                            self.last_score = score;
                            break;
                        }

                        if delta > 100_000 {
                            alpha = -999_999_999;
                            beta = 999_999_999;
                        }
                    }
                }

                if self.stop {
                    break;
                }
            }

            let elapsed = start.elapsed().as_millis();
            let score = self.last_score;

            let score_str = if score > 900_000 {
                let plies = 1_900_000 - score;
                format!("mate {}", (plies + 1) / 2)
            } else if score < -900_000 {
                let plies = 1_900_000 + score;
                format!("mate -{}", (plies + 1) / 2)
            } else {
                format!("cp {}", score)
            };
            println!(
                "info depth {} score {} nodes {} time {}",
                depth, score_str, self.nodes, elapsed
            );

            if score >= 900000 || score <= -900000 {
                break;
            }

            if self.stop {
                break;
            }

            score_history.push(self.last_score);
        }

        best_move
    }

    fn search_at_depth(
        &mut self,
        depth: u8,
        mut alpha: i32,
        beta: i32,
        start: &Instant,
        deadline: u64,
    ) -> Option<(Move, i32)> {
        let color = if self.game.white_turn {
            Color::White
        } else {
            Color::Black
        };

        let moves = {
            let move_gen = MoveGen::new(&self.game);
            move_gen.pseudo_legal_moves(color)
        };
        let hash = self.game.hash;
        let mut moves = MoveGen::filter_legal(moves, &mut self.game, color);

        if moves.is_empty() {
            return None;
        }

        let tt_idx = (hash as usize) & (self.tt.len() - 1);
        if let Some(entry) = self.tt[tt_idx] {
            if entry.hash == hash {
                if let Some(tt_move) = entry.best_move {
                    if let Some(pos) = moves
                        .iter()
                        .position(|m| m.from == tt_move.from && m.to == tt_move.to)
                    {
                        moves.swap(0, pos);
                    }
                }
            }
        }
        moves[1..].sort_by_key(|m| -self.move_score(m, 0, color));

        // Track best regardless of whether it beats alpha — needed for fail-low detection
        let mut best_move = moves[0];
        let mut best_score = i32::MIN;

        for mv in moves.iter() {
            if start.elapsed().as_millis() as u64 >= deadline {
                self.stop = true;
                return None; // genuine timeout
            }

            let undo = self.game.make_move(mv);
            let score = -self.negamax(depth - 1, -beta, -alpha, start, deadline, true, 0);
            self.game.undo_move(&undo);

            if self.stop {
                return None;
            }

            if score > best_score {
                best_score = score;
                best_move = *mv;
            }

            if score > alpha {
                alpha = score;
            }

            if alpha >= beta {
                break; // fail-high cutoff
            }
        }

        Some((best_move, best_score))
    }

    fn negamax(
        &mut self,
        mut depth: u8,
        mut alpha: i32,
        mut beta: i32,
        start: &Instant,
        deadline: u64,
        can_null: bool,
        ply: u8,
    ) -> i32 {
        if self.nodes & 2047 == 0 {
            if start.elapsed().as_millis() as u64 >= deadline {
                self.stop = true;
            }
        }

        if self.stop {
            return 0;
        }

        if self.game.fifty_move_rule >= 100 {
            return 0;
        }

        self.nodes += 1;
        let hash = self.game.hash;
        let search_count = self.history.get(&hash).copied().unwrap_or(0);
        let game_count = self.game_history.get(&hash).copied().unwrap_or(0);

        if search_count >= 2 || game_count >= 2 {
            self.refresh_cache();
            let eval = self.static_eval();
            return if eval > 0 { -50 } else { 50 };
        }

        let original_alpha = alpha;
        let tt_idx = (hash as usize) & (self.tt.len() - 1);

        if let Some(entry) = self.tt[tt_idx] {
            if entry.hash == hash && entry.depth >= depth {
                let tt_score = score_from_tt(entry.score, ply);
                match entry.flag {
                    TTFlag::Exact => return tt_score,
                    TTFlag::LowerBound => alpha = alpha.max(tt_score),
                    TTFlag::UpperBound => beta = beta.min(tt_score),
                }
                if alpha >= beta {
                    return tt_score;
                }
            }
        }

        let color = if self.game.white_turn {
            Color::White
        } else {
            Color::Black
        };

        let in_check = self.game.board_collection.check_info(color).in_check;

        depth = if in_check { depth + 1 } else { depth };

        if depth == 0 {
            return self.quiescence(alpha, beta);
        }

        // Null move pruning

        if can_null && !in_check && depth >= 3 && beta < 900_000 {
            let bc = self.game.board_collection;
            let has_non_pawns = (bc.occupied_color(color).0
                & !(bc.get_board(&color.kind(PieceKind::Pawn)).0
                    | bc.get_board(&color.kind(PieceKind::King)).0))
                != 0;

            if has_non_pawns {
                self.refresh_cache();
                let static_eval = self.static_eval();

                if static_eval >= beta {
                    let r = 3 + (depth / 6) as u8 + if static_eval - beta > 200 { 1 } else { 0 };
                    let reduced_depth = depth.saturating_sub(1 + r);

                    let prev_hash = self.game.hash;
                    self.game.white_turn = !self.game.white_turn;
                    self.game.hash ^= zobrist().black_to_move;
                    let prev_ep = self.game.en_passant_square;
                    if let Some(ep) = self.game.en_passant_square {
                        let (file, _) = BC::decode_tile(ep);
                        self.game.hash ^= zobrist().en_passant[file as usize];
                    }

                    self.game.en_passant_square = None;

                    let score = -self.negamax(
                        reduced_depth,
                        -beta,
                        -beta + 1,
                        start,
                        deadline,
                        false,
                        ply + 1,
                    );

                    self.game.white_turn = !self.game.white_turn;
                    self.game.en_passant_square = prev_ep;
                    self.game.hash = prev_hash;
                    self.cache_hash = 0; // force cache refresh

                    // Position so good, that oponent wouldnt let us reach it
                    if score >= beta {
                        // Verification search at high depth to catch zugzwang
                        if depth >= 10 {
                            let verify = self.negamax(
                                depth - r - 1,
                                beta - 1,
                                beta,
                                start,
                                deadline,
                                false,
                                ply + 1,
                            );
                            if verify >= beta {
                                return score; // fail-soft
                            }
                            // verification failed — fall through to normal search
                        } else {
                            // Don't return mate scores from null move
                            if score >= 900_000 {
                                return beta;
                            }
                            return score;
                        }
                    }
                }
            }
        }

        let moves = {
            let move_gen = MoveGen::new(&self.game);

            let pseudo = move_gen.pseudo_legal_moves(color);
            pseudo
        };

        let mut moves = MoveGen::filter_legal(moves, &mut self.game, color);

        if moves.is_empty() {
            if in_check {
                return -(1900000 - ply as i32);
            }
            return 0;
        }

        let tt_idx = (hash as usize) & (self.tt.len() - 1);
        if let Some(entry) = self.tt[tt_idx] {
            if entry.hash == hash {
                if let Some(tt_move) = entry.best_move {
                    if let Some(pos) = moves
                        .iter()
                        .position(|m| m.from == tt_move.from && m.to == tt_move.to)
                    {
                        moves.swap(0, pos);
                    }
                }
            }
        }

        // sort the rest
        moves[1..].sort_by_key(|m| -self.move_score(m, ply, color));

        *self.history.entry(hash).or_insert(0) += 1;
        let mut best_move = None;

        for (i, m) in moves.iter().enumerate() {
            let u = self.game.make_move(m);

            let score = if i == 0 {
                // full search window
                -self.negamax(depth - 1, -beta, -alpha, start, deadline, can_null, ply + 1)
            } else {
                // late move reduction (LMR), look less ahead and tigther
                let mut score = if i >= 3
                    && depth >= 3
                    && !m.flags.contains(MoveFlags::CAPTURE)
                    && !m.flags.contains(MoveFlags::PROMOTE_Q)
                    && !self.game.board_collection.is_in_check(!color)
                {
                    let r = ((depth as f64).ln() * (i as f64).ln() / 2.0) as u8;
                    let r = r.max(1);
                    let reduced_depth = (depth - 1).saturating_sub(r);

                    -self.negamax(
                        reduced_depth,
                        -alpha - 1,
                        -alpha,
                        start,
                        deadline,
                        true,
                        ply + 1,
                    )
                } else {
                    alpha + 1
                };

                if score > alpha {
                    // regular depth but tighter
                    score = -self.negamax(
                        depth - 1,
                        -alpha - 1,
                        -alpha,
                        start,
                        deadline,
                        true,
                        ply + 1,
                    );
                }

                if score > alpha && score < beta {
                    // actual full search
                    score = -self.negamax(depth - 1, -beta, -alpha, start, deadline, true, ply + 1);
                }

                score
            };
            self.game.undo_move(&u);

            if score >= beta {
                // Move too good
                if !m.flags.contains(MoveFlags::CAPTURE)
                    && !m.flags.intersects(
                        MoveFlags::PROMOTE_Q
                            | MoveFlags::PROMOTE_R
                            | MoveFlags::PROMOTE_N
                            | MoveFlags::PROMOTE_B,
                    )
                {
                    let bonus = (depth as i32) * (depth as i32);
                    let color_idx = color as usize;
                    self.history_table[color_idx][m.from as usize][m.to as usize] += bonus;

                    for prev in moves[..i].iter() {
                        if !prev.flags.contains(MoveFlags::CAPTURE)
                            && !prev.flags.intersects(
                                MoveFlags::PROMOTE_Q
                                    | MoveFlags::PROMOTE_R
                                    | MoveFlags::PROMOTE_N
                                    | MoveFlags::PROMOTE_B,
                            )
                        {
                            self.history_table[color_idx][prev.from as usize][prev.to as usize] -=
                                bonus;
                        }
                    }

                    let dominated = self.killers[ply as usize][0];
                    let dominated_matches = dominated
                        .map(|k| k.from == m.from && k.to == m.to)
                        .unwrap_or(false);

                    if !dominated_matches {
                        self.killers[ply as usize][1] = self.killers[ply as usize][0];
                        self.killers[ply as usize][0] = Some(*m);
                    }
                }

                self.tt[tt_idx] = Some(TTEntry {
                    hash,
                    depth,
                    score: score_to_tt(score, ply),
                    flag: TTFlag::LowerBound,
                    best_move: Some(*m),
                });
                if let Some(count) = self.history.get_mut(&hash) {
                    *count -= 1;
                    if *count == 0 {
                        self.history.remove(&hash);
                    }
                }
                return score;
            }

            if score > alpha {
                alpha = score;
                best_move = Some(*m);
            }
        }

        let flag = if alpha >= beta {
            TTFlag::LowerBound
        } else if alpha > original_alpha {
            TTFlag::Exact
        } else {
            TTFlag::UpperBound
        };

        self.tt[tt_idx] = Some(TTEntry {
            hash,
            depth,
            score: score_to_tt(alpha, ply),
            flag,
            best_move,
        });

        if let Some(count) = self.history.get_mut(&hash) {
            *count -= 1;
            if *count == 0 {
                self.history.remove(&hash);
            }
        }

        alpha
    }

    fn move_score(&self, m: &Move, ply: u8, color: Color) -> i32 {
        if m.flags.contains(MoveFlags::CAPTURE) {
            let see_value = self.see_for_move(m);
            // Winning captures > killers, losing captures < quiet history
            if see_value >= 0 {
                return 10000 + see_value;
            } else {
                return -10000 + see_value; // still ordered among themselves, but below quiets
            }
        }
        if m.flags.intersects(MoveFlags::PROMOTE_Q) {
            return 9000;
        }
        if let Some(k) = self.killers[ply as usize][0] {
            if k.from == m.from && k.to == m.to {
                return 8000;
            }
        }
        if let Some(k) = self.killers[ply as usize][1] {
            if k.from == m.from && k.to == m.to {
                return 7000;
            }
        }
        self.history_table[color as usize][m.from as usize][m.to as usize]
    }

    fn see_for_move(&self, m: &Move) -> i32 {
        if m.flags.contains(MoveFlags::EN_PASSANT) {
            return 0;
        }

        let bc = &self.game.board_collection;
        let target = bc.piece_at_index(m.to).unwrap();
        let attacker = bc.piece_at_index(m.from).unwrap();
        self.see(m.to, target.kind.value(), m.from, attacker.kind.value())
    }

    fn quiescence(&mut self, mut alpha: i32, beta: i32) -> i32 {
        let hash = self.game.hash;
        let search_count = self.history.get(&hash).copied().unwrap_or(0);
        let game_count = self.game_history.get(&hash).copied().unwrap_or(0);

        if search_count >= 1 || game_count >= 2 {
            return 0;
        }

        let color = if self.game.white_turn {
            Color::White
        } else {
            Color::Black
        };

        self.refresh_cache();
        let stand_pat = self.static_eval();

        if stand_pat >= beta {
            return stand_pat;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        if stand_pat + 200 < alpha {
            return alpha;
        }

        let move_gen = MoveGen::new(&self.game);
        let moves = move_gen
            .pseudo_legal_moves(color)
            .into_iter()
            .filter(|m| m.flags.contains(MoveFlags::CAPTURE) && self.see_for_move(m) >= 0)
            .collect::<Vec<_>>();

        let mut moves = MoveGen::filter_legal(moves, &mut self.game, color);
        moves.sort_by_key(|m| -self.see_for_move(&m));

        for m in moves {
            let u = self.game.make_move(&m);
            let score = -self.quiescence(-beta, -alpha);
            self.game.undo_move(&u);

            if score >= beta {
                return score;
            }
            if score > alpha {
                alpha = score;
            }
        }

        alpha
    }

    fn debug_eval(&mut self) {
        self.refresh_cache();

        let color = if self.game.white_turn {
            Color::White
        } else {
            Color::Black
        };
        let bc = &self.game.board_collection;
        let friendly_attacks = self.cached_attacks[color as usize];
        let enemy_attacks = self.cached_attacks[(!color) as usize];
        let friendly_pieces = bc.occupied_color(color);
        let enemy_pieces = bc.occupied_color(!color);

        let mut mg = [0i32; 2];
        let mut eg = [0i32; 2];
        let mut game_phase = 0i32;
        let mut white_king_sq = 0u8;
        let mut black_king_sq = 0u8;
        let mut friendly_bishops = 0u8;
        let mut enemy_bishops = 0u8;

        for sq in 0..64u8 {
            if let Some(piece) = bc.piece_at_index(sq) {
                let color_idx = piece.color as usize;
                let table_sq = if piece.color == Color::White {
                    sq as usize
                } else {
                    (sq ^ 56) as usize
                };

                let (mg_val, eg_val) = match piece.kind {
                    PieceKind::Pawn => (MG_PAWN_TABLE[table_sq], EG_PAWN_TABLE[table_sq]),
                    PieceKind::Knight => (MG_KNIGHT_TABLE[table_sq], EG_KNIGHT_TABLE[table_sq]),
                    PieceKind::Bishop => (MG_BISHOP_TABLE[table_sq], EG_BISHOP_TABLE[table_sq]),
                    PieceKind::Rook => (MG_ROOK_TABLE[table_sq], EG_ROOK_TABLE[table_sq]),
                    PieceKind::Queen => (MG_QUEEN_TABLE[table_sq], EG_QUEEN_TABLE[table_sq]),
                    PieceKind::King => (MG_KING_TABLE[table_sq], EG_KING_TABLE[table_sq]),
                };

                let material = match piece.kind {
                    PieceKind::Pawn => 100,
                    PieceKind::Knight => 300,
                    PieceKind::Bishop => 300,
                    PieceKind::Rook => 500,
                    PieceKind::Queen => 900,
                    PieceKind::King => 0,
                };

                if piece.kind == PieceKind::King {
                    if piece.color == Color::White {
                        white_king_sq = sq;
                    } else {
                        black_king_sq = sq;
                    }
                }
                if piece.kind == PieceKind::Bishop {
                    if piece.color == color {
                        friendly_bishops += 1;
                    } else {
                        enemy_bishops += 1;
                    }
                }

                mg[color_idx] += mg_val + material;
                eg[color_idx] += eg_val + material;
                game_phase += GAMEPHASE_INC[piece.kind as usize];
            }
        }

        let side = if self.game.white_turn { 0 } else { 1 };
        let other = 1 - side;
        let mg_phase = game_phase.min(24);
        let eg_phase = 24 - mg_phase;

        let pst_score =
            (mg[side] - mg[other]) * mg_phase / 24 + (eg[side] - eg[other]) * eg_phase / 24;

        let friendly_pawns = *bc.get_board(&color.kind(PieceKind::Pawn));
        let enemy_pawns = *bc.get_board(&(!color).kind(PieceKind::Pawn));

        let king_sq = if self.game.white_turn {
            white_king_sq
        } else {
            black_king_sq
        };
        let enemy_king_sq = if self.game.white_turn {
            black_king_sq
        } else {
            white_king_sq
        };

        let hanging = self.hanging_penalty(enemy_attacks, friendly_attacks, friendly_pieces)
            - self.hanging_penalty(friendly_attacks, enemy_attacks, enemy_pieces);

        let king_safety = (self.king_safety(king_sq, friendly_pieces, enemy_attacks)
            - self.king_safety(enemy_king_sq, enemy_pieces, friendly_attacks))
            * mg_phase
            / 24;
        let pawns = self.pawn_bonus(friendly_pawns, enemy_pawns, color)
            - self.pawn_bonus(enemy_pawns, friendly_pawns, !color);
        let bishops = self.bishop_bonus(friendly_bishops) - self.bishop_bonus(enemy_bishops);
        let castling = {
            let our = if self.game.white_turn {
                self.game.k_white as i32 + self.game.q_white as i32
            } else {
                self.game.k_black as i32 + self.game.q_black as i32
            };
            let their = if self.game.white_turn {
                self.game.k_black as i32 + self.game.q_black as i32
            } else {
                self.game.k_white as i32 + self.game.q_white as i32
            };
            (our - their) * 20
        };

        println!("=== Eval Breakdown ===");
        println!("PST + material: {}", pst_score);

        println!("king_safety:    {}", king_safety);
        let a = self.pawn_bonus(friendly_pawns, enemy_pawns, color);
        let b = self.pawn_bonus(enemy_pawns, friendly_pawns, !color);
        println!("pawn bonus friendly: {} enemy: {} diff: {}", a, b, a - b);
        println!("bishops:        {}", bishops);
        println!("castling:       {}", castling);
        println!(
            "total:          {}",
            pst_score + king_safety + pawns + bishops + castling
        );
        println!("Hash: {}", self.game.hash);
        println!("Hash check: {}", zobrist().hash(&self.game));
    }

    fn material_score(&self, color: Color) -> i32 {
        let mut score = 0;
        for sq in 0..64u8 {
            if let Some(piece) = self.game.board_collection.piece_at_index(sq) {
                if piece.color == color {
                    score += piece.kind.value();
                }
            }
        }
        score as i32
    }
}

fn score_to_tt(score: i32, ply: u8) -> i32 {
    if score > 900_000 {
        score + ply as i32
    } else if score < -900_000 {
        score - ply as i32
    } else {
        score
    }
}

fn score_from_tt(score: i32, ply: u8) -> i32 {
    if score > 900_000 {
        score - ply as i32
    } else if score < -900_000 {
        score + ply as i32
    } else {
        score
    }
}

fn clear() {
    print!("\x1B[2J\x1B[1;1H");
}

fn take_input() -> String {
    let mut input = String::new();

    io::stdin().read_line(&mut input).unwrap_or_default();

    input.trim().to_string()
}

const START_POS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
fn main() {
    let mut engine = Engine::new();

    loop {
        let input = take_input();
        if input == "quit" {
            std::process::exit(0);
        }

        if input == "uci" {
            println!("id name BadFish");
            println!("id author DarkoS");
            println!("uciok");
        }

        if input == "isready" {
            println!("readyok");
        }

        if input == "stop" {
            engine.stop = true;
        }

        if input == "ucinewgame" {
            engine = Engine::new();
        }

        if input == "eval" {
            engine.debug_eval();
        }

        if input.starts_with("go perft") {
            let track = Instant::now();
            let n = input.split_whitespace().collect::<Vec<&str>>()[2]
                .parse::<u8>()
                .unwrap_or(0);
            println!(
                "\ntotal: {} ({}s)",
                count_positions_n_deep(n, &mut engine.game, true),
                track.elapsed().as_secs_f32()
            );
            continue;
        }

        if input.starts_with("go") {
            engine.stop = false;
            let parts = input.split_whitespace().collect::<Vec<&str>>();
            let depth = if parts.len() > 2 && parts[1] == "depth" {
                parts[2].parse::<u8>().unwrap_or(4)
            } else {
                64
            };

            let time_ms = if engine.game.white_turn {
                parts
                    .windows(2)
                    .find(|w| w[0] == "wtime")
                    .and_then(|w| w[1].parse::<u64>().ok())
            } else {
                parts
                    .windows(2)
                    .find(|w| w[0] == "btime")
                    .and_then(|w| w[1].parse::<u64>().ok())
            }
            .map(|t| t / 20) // use 1/20th of remaining time per move
            .unwrap_or(5000);

            if let Some(mv) = engine.search(depth, time_ms) {
                let mut mv_s = BC::encode_notation(mv.from);
                mv_s.extend(BC::encode_notation(mv.to).chars());
                let promo = if mv.flags.contains(MoveFlags::PROMOTE_Q) {
                    "q"
                } else if mv.flags.contains(MoveFlags::PROMOTE_R) {
                    "r"
                } else if mv.flags.contains(MoveFlags::PROMOTE_N) {
                    "n"
                } else if mv.flags.contains(MoveFlags::PROMOTE_B) {
                    "b"
                } else {
                    ""
                };

                println!("bestmove {}{}", mv_s, promo);
            }
        }

        if input.starts_with("position") {
            engine.game_history.clear();
            let parts = input.split_ascii_whitespace().collect::<Vec<&str>>();
            let mut idx = 1;
            let mut new_game;
            if parts[idx] == "startpos" {
                new_game = Game::from_fen(START_POS);
            } else if parts[idx] == "fen" {
                new_game = Game::from_fen(parts[2..=7].join(" ").as_str());
                idx = 7;
            } else {
                continue;
            }
            idx += 1;

            if parts.len() - 1 < idx || parts[idx] != "moves" {
                engine.game = new_game;
                continue;
            }
            idx += 1;

            for mv in parts[idx..].iter() {
                if mv.len() < 4 {
                    continue;
                }
                let from = BC::decode_notation(&mv[0..2]);
                let to = BC::decode_notation(&mv[2..4]);
                let promo = mv.chars().nth(4);
                let color = if new_game.white_turn {
                    Color::White
                } else {
                    Color::Black
                };

                let move_gen = MoveGen::new(&new_game);
                let actual_move = move_gen
                    .pseudo_legal_moves(color)
                    .into_iter()
                    .filter(|m| {
                        let undo = new_game.make_move(m);
                        let legal = !new_game.board_collection.is_in_check(color);
                        new_game.undo_move(&undo);
                        legal
                    })
                    .find(|m| {
                        m.from == from
                            && m.to == to
                            && match promo {
                                Some('q') => m.flags.contains(MoveFlags::PROMOTE_Q),
                                Some('r') => m.flags.contains(MoveFlags::PROMOTE_R),
                                Some('n') => m.flags.contains(MoveFlags::PROMOTE_N),
                                Some('b') => m.flags.contains(MoveFlags::PROMOTE_B),
                                None => true,
                                _ => false,
                            }
                    });

                if let Some(m) = actual_move {
                    new_game.make_move(&m);
                    *engine.game_history.entry(new_game.hash).or_insert(0) += 1;
                }
            }

            engine.game = new_game;
        }

        if input.starts_with("full perft") {
            for n in 0..7 {
                let track = Instant::now();
                println!(
                    "{}: {} ({}s)",
                    n + 1,
                    count_positions_n_deep(n + 1, &mut engine.game, false),
                    track.elapsed().as_secs_f32()
                )
            }
        }

        if input == "d" {
            println!("{}", engine.game.board_collection);
            println!("Fen: {}", engine.game.into_fen());
        }
    }
}

// Perft function
fn count_positions_n_deep(n: u8, game: &mut Game, split: bool) -> u32 {
    if n == 0 {
        return 1;
    }

    let mut s = 0;
    let color = if game.white_turn {
        Color::White
    } else {
        Color::Black
    };

    let moves = {
        let move_gen = MoveGen::new(game);
        let pseudo = move_gen.pseudo_legal_moves(color);
        pseudo
    };

    let moves = MoveGen::filter_legal(moves, game, color);

    for mv in moves.iter() {
        let undo = game.make_move(mv);
        let m = count_positions_n_deep(n - 1, game, false);
        game.undo_move(&undo);
        s += m;

        if split {
            let mut mv_s = BC::encode_notation(mv.from);
            mv_s.extend(BC::encode_notation(mv.to).chars());
            println!("{}: {}", mv_s, m);
        }
    }

    s
}

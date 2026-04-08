use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
    io,
    ops::{BitAnd, BitOr, Not, RangeInclusive},
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
    // TODO: pin bitboard, and yeah
}

use BitBoardCollection as BC;

impl BitBoardCollection {
    fn new() -> Self {
        Self {
            piece_boards: [[BitBoard(0); 6]; 2],
        }
    }

    fn get_board(&self, piece: &ChessPiece) -> &BitBoard {
        &self.piece_boards[piece.color as usize][piece.kind as usize]
    }
    fn get_board_mut(&mut self, piece: &ChessPiece) -> &mut BitBoard {
        &mut self.piece_boards[piece.color as usize][piece.kind as usize]
    }

    fn insert(&mut self, index: u8, piece: &ChessPiece) {
        self.get_board_mut(piece).insert(index);
    }

    fn remove(&mut self, index: u8, piece: &ChessPiece) {
        self.get_board_mut(piece).remove(index);
    }

    fn contains(&mut self, index: u8, piece: &ChessPiece) -> bool {
        self.get_board_mut(piece).contains(index)
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
            writeln!(f)?;
        }
        writeln!(f, "+---+---+---+---+---+---+---+---+")?;

        Ok(())
    }
}

// -- Moves --

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
    bc: &'a BitBoardCollection,
    en_passant: Option<u8>,
}
// attack = threats/protections
// quiets = empty spaces that the piece can move to
// captures = attack & opposite_color
// protections = attack & same_color
// moves = quiets | captures
// lists need to be split up in quiets and captures, pawns have special double push for en_passant tracking
impl<'a> MoveGen<'a> {
    fn new(game: &'a Game) -> Self {
        Self {
            bc: &game.board_collection,
            en_passant: game.en_passant_square,
        }
    }

    fn piece_attacks(&self, index: u8, piece: ChessPiece) -> BitBoard {
        let color = piece.color;
        match piece.kind {
            PieceKind::Pawn => self.pawn_attacks(index, color),
            PieceKind::Knight => self.knight_attacks(index, color),
            PieceKind::Bishop => self.bishop_attacks(index, color),
            PieceKind::Rook => self.rook_attacks(index, color),
            PieceKind::Queen => self.bishop_attacks(index, color) | self.rook_attacks(index, color),
            PieceKind::King => self.king_attacks(index, color),
        }
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

    fn all_attacked_by(&self, color: Color) -> BitBoard {
        let mut attacks = BitBoard(0);

        for kind in (0..6) {
            let piece = ChessPiece {
                kind: kind.try_into().unwrap(),
                color,
            };
            let mut bb = *self.bc.get_board(&piece);

            while let Some(index) = bb.pop_lsb() {
                attacks.0 |= self.piece_attacks(index, piece).0;
            }
        }

        attacks
    }

    // -- Pawn --
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
    fn pawn_en_passant(&self, index: u8, color: Color) -> BitBoard {
        // En passant is a capture, but also a threat.

        let en_passant = self
            .en_passant
            .map(|ep| BitBoard(1 << ep))
            .unwrap_or(BitBoard(0));

        en_passant
    }
    fn pawn_captures(&self, index: u8, color: Color) -> BitBoard {
        self.pawn_attacks(index, color)
            & (self.pawn_en_passant(index, color) | self.bc.occupied_color(!color))
    }
    fn pawn_single(&self, index: u8, color: Color) -> BitBoard {
        let forward = color.pawn_forward();
        let single = BitBoard(0b1 << index as i16 + forward) & !self.bc.occupied();

        single
    }
    fn pawn_quiets(&self, index: u8, color: Color) -> BitBoard {
        // Pawn push seperate because pawns move differently than when they capture, pushes are moves without captures.
        let start_rank_range = color.pawn_start_rank();
        let forward = color.pawn_forward();
        let single = self.pawn_single(index, color);

        let double = if start_rank_range.contains(&index) && !single.is_empty() {
            BitBoard(0b1 << (index as i16 + forward * 2)) & !self.bc.occupied()
        } else {
            BitBoard(0)
        };

        single | double
    }
    fn pawn_moves(&self, index: u8, color: Color) -> BitBoard {
        self.pawn_quiets(index, color) | self.pawn_captures(index, color)
    }

    fn pawn_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        let base_move = Move::new(index, 0, MoveFlags::QUIET);
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
            if !promo_rank.contains(&to) {
                moves.push(base_move.modified(to, MoveFlags::QUIET | MoveFlags::DOUBLE_PAWN_PUSH));
            } else {
                for t in [
                    MoveFlags::PROMOTE_Q,
                    MoveFlags::PROMOTE_R,
                    MoveFlags::PROMOTE_N,
                    MoveFlags::PROMOTE_B,
                ] {
                    moves.push(
                        base_move.modified(to, MoveFlags::QUIET | t | MoveFlags::DOUBLE_PAWN_PUSH),
                    )
                }
            }
        }

        let mut captures = self.pawn_captures(index, color);

        while let Some(to) = captures.pop_lsb() {
            if !promo_rank.contains(&to) {
                moves.push(base_move.modified(to, MoveFlags::CAPTURE));
            } else {
                for t in [
                    MoveFlags::PROMOTE_Q,
                    MoveFlags::PROMOTE_R,
                    MoveFlags::PROMOTE_N,
                    MoveFlags::PROMOTE_B,
                ] {
                    moves.push(base_move.modified(to, MoveFlags::CAPTURE | t))
                }
            }
        }

        moves
    }

    // -- Knight --
    fn knight_attacks(&self, index: u8, color: Color) -> BitBoard {
        let (file, rank) = BC::decode_tile(index);
        let mut attack = BitBoard(0);
        for (forward, right) in KNIGHT_DIRECTIONS {
            let new_file = file as i8 + forward;
            let new_rank = rank as i8 + right;

            if !(0..8).contains(&new_file) || !(0..8).contains(&new_rank) {
                continue;
            }

            attack.0 |= 1 << BC::encode_tile(new_file as u8, new_rank as u8);
        }

        attack
    }
    fn knight_captures(&self, index: u8, color: Color) -> BitBoard {
        self.knight_attacks(index, color) & self.bc.occupied_color(!color)
    }
    fn knight_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.knight_attacks(index, color) & !self.bc.occupied()
    }
    fn knight_moves(&self, index: u8, color: Color) -> BitBoard {
        self.knight_quiets(index, color) | self.knight_captures(index, color)
    }

    // -- King --
    fn king_attacks(&self, index: u8, color: Color) -> BitBoard {
        let (file, rank) = BC::decode_tile(index);
        let mut attack = BitBoard(0);

        for (f, r) in KING_DIRECTIONS {
            let new_file = file as i8 + f;
            let new_rank = rank as i8 + r;

            if !(0..8).contains(&new_file) || !(0..8).contains(&new_rank) {
                continue;
            }

            attack.0 |= 1 << BC::encode_tile(new_file as u8, new_rank as u8)
        }

        attack
    }
    fn king_captures(&self, index: u8, color: Color) -> BitBoard {
        self.king_attacks(index, color)
            & self.bc.occupied_color(!color)
            & !(self.all_attacked_by(!color))
    }
    fn king_quiets(&self, index: u8, color: Color) -> BitBoard {
        // King cant move to an attacked square
        self.king_attacks(index, color) & !(self.all_attacked_by(!color)) & !self.bc.occupied()
    }
    fn king_moves(&self, index: u8, color: Color) -> BitBoard {
        self.king_quiets(index, color) | self.king_captures(index, color)
    }

    // -- Rook --
    fn rook_attacks(&self, index: u8, color: Color) -> BitBoard {
        let (file, rank) = BC::decode_tile(index);
        let mut attack = BitBoard(0);

        for (f, r) in ROOK_DIRECTIONS {
            let mut new_file = file as i8;
            let mut new_rank = rank as i8;

            loop {
                new_file += f;
                new_rank += r;
                if !(0..8).contains(&new_file) || !(0..8).contains(&new_rank) {
                    break;
                }

                let tile = BC::encode_tile(new_file as u8, new_rank as u8);
                attack.0 |= 1 << tile;

                if self.bc.occupied().contains(tile) {
                    break;
                }
            }
        }

        attack
    }
    fn rook_captures(&self, index: u8, color: Color) -> BitBoard {
        self.rook_attacks(index, color) & self.bc.occupied_color(!color)
    }
    fn rook_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.rook_attacks(index, color) & !self.bc.occupied()
    }
    fn rook_moves(&self, index: u8, color: Color) -> BitBoard {
        self.rook_quiets(index, color) | self.rook_captures(index, color)
    }

    // -- Bishop --
    fn bishop_attacks(&self, index: u8, color: Color) -> BitBoard {
        let (file, rank) = BC::decode_tile(index);
        let mut attack = BitBoard(0);

        for (f, r) in BISHOP_DIRECTIONS {
            let mut new_file = file as i8;
            let mut new_rank = rank as i8;

            loop {
                new_file += f;
                new_rank += r;

                if !(0..8).contains(&new_file) || !(0..8).contains(&new_rank) {
                    break;
                }
                let tile = BC::encode_tile(new_file as u8, new_rank as u8);
                attack.0 |= 1 << tile;

                if self.bc.occupied().contains(tile) {
                    break;
                }
            }
        }

        attack
    }
    fn bishop_captures(&self, index: u8, color: Color) -> BitBoard {
        self.bishop_attacks(index, color) & self.bc.occupied_color(!color)
    }
    fn bishop_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.bishop_attacks(index, color) & !self.bc.occupied()
    }
    fn bishop_moves(&self, index: u8, color: Color) -> BitBoard {
        self.bishop_quiets(index, color) | self.bishop_captures(index, color)
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
                // "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                "r2qk2r/2p2ppp/p1n1bn2/1p1pp3/3P4/2N1PN2/PPP1BPPP/R2QK2R w KQkq - 0 9",
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
        // let move_gen = MoveGen::new(&game);
        // println!(
        //     "{}",
        //     move_gen.all_attacked_by(Color::White) & move_gen.bc.occupied_color(Color::Black)
        // );
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

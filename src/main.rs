use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
    io, mem,
    ops::{BitAnd, BitOr, Index, Not, RangeInclusive},
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

use BitBoardCollection as BC;

impl BitBoardCollection {
    fn new() -> Self {
        Self {
            piece_boards: [[BitBoard(0); 6]; 2],
            mailbox: [None; 64],
        }
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
        };

        mg.black_attacks = mg.compute_attacks(Color::Black);

        mg.white_attacks = mg.compute_attacks(Color::White);

        mg
    }

    fn compute_attacks(&self, color: Color) -> BitBoard {
        let mut attacks = BitBoard(0);

        for kind in 0..6 {
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

    fn legal_moves(&self, color: Color) -> Vec<Move> {
        let mut moves = Vec::new();
        let mut clone_game = self.game.clone();

        for mv in self.pseudo_legal_moves(color) {
            let undo = clone_game.make_move(&mv);
            let move_gen = MoveGen::new(&clone_game);
            if !move_gen.is_in_check(color) {
                moves.push(mv);
            }
            clone_game.undo_move(&undo);
        }

        moves
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
            .game
            .en_passant_square
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
        let single = BitBoard(0b1 << (index as i16 + forward)) & !self.bc.occupied();

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
            & !(self.attacks_by(!color))
    }
    fn king_quiets(&self, index: u8, color: Color) -> BitBoard {
        // King cant move to an attacked square
        self.king_attacks(index, color) & !(self.attacks_by(!color)) & !self.bc.occupied()
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
        let occupied = self.bc.occupied();
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

    fn is_in_check(&self, color: Color) -> bool {
        self.bc
            .get_board(&ChessPiece {
                kind: PieceKind::King,
                color,
            })
            .0
            & self.attacks_by(!color).0
            != 0
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

        Self {
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
        }
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

        let color = if self.white_turn {
            Color::White
        } else {
            Color::Black
        };

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
            }
        }

        self.board_collection.remove(m.from, &piece_from);

        if !is_promotion {
            self.board_collection.insert(m.to, &piece_from);
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

            self.board_collection.insert(
                m.to,
                &ChessPiece {
                    kind: new_kind,
                    color,
                },
            );
        }

        if m.flags.contains(MoveFlags::EN_PASSANT) {
            let target = m.to as i16 - color.pawn_forward();
            let captured_pawn = ChessPiece {
                kind: PieceKind::Pawn,
                color: !color,
            };

            undo_move.piece_captured = Some(captured_pawn);
            self.board_collection.remove(target as u8, &captured_pawn);
        }

        if m.flags.contains(MoveFlags::DOUBLE_PAWN_PUSH) {
            self.en_passant_square = Some((m.from as i16 + color.pawn_forward()) as u8)
        }

        if m.flags.contains(MoveFlags::CASTLE_KING) {
            let rook_index: u8 = if self.white_turn { 7 } else { 63 };
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };
            self.board_collection.remove(rook_index, &rook);
            self.board_collection.insert(m.from + 1, &rook);
        }

        if m.flags.contains(MoveFlags::CASTLE_QUEEN) {
            let rook_index: u8 = if self.white_turn { 0 } else { 56 };
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };
            self.board_collection.remove(rook_index, &rook);
            self.board_collection.insert(m.from - 1, &rook);
        }

        if piece_from.kind == PieceKind::King {
            if self.white_turn {
                self.k_white = false;
                self.q_white = false;
            } else {
                self.k_black = false;
                self.q_black = false;
            }
        }

        if m.from == 0 || m.to == 0 {
            self.q_white = false;
        }

        if m.from == 7 || m.to == 7 {
            self.k_white = false;
        }

        if m.from == 56 || m.to == 56 {
            self.q_black = false;
        }

        if m.from == 63 || m.to == 63 {
            self.k_black = false;
        }

        self.white_turn = !self.white_turn;

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

        let piece_from = self.board_collection.piece_at_index(u.mv.to).unwrap();
        self.board_collection.remove(u.mv.to, &piece_from);
        self.board_collection.insert(u.mv.from, &piece_from);

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

        if u.mv.flags.intersects(
            MoveFlags::PROMOTE_Q
                | MoveFlags::PROMOTE_R
                | MoveFlags::PROMOTE_N
                | MoveFlags::PROMOTE_B,
        ) {
            self.board_collection.remove(u.mv.from, &piece_from);
            self.board_collection.insert(
                u.mv.from,
                &ChessPiece {
                    kind: PieceKind::Pawn,
                    color,
                },
            );
        }

        let prev = u.rights;
        self.k_white = prev & 1 != 0;
        self.q_white = prev & 2 != 0;
        self.k_black = prev & 4 != 0;
        self.q_black = prev & 8 != 0;

        // King already moved, just need to move rook
        if u.mv.flags.contains(MoveFlags::CASTLE_KING) {
            // King side, so rook is left of king
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };

            self.board_collection.remove(u.mv.to - 1, &rook);
            self.board_collection.insert(u.mv.to + 1, &rook);
        }

        if u.mv.flags.contains(MoveFlags::CASTLE_QUEEN) {
            // King side, so rook is left of king
            let rook = ChessPiece {
                kind: PieceKind::Rook,
                color,
            };

            self.board_collection.remove(u.mv.to + 1, &rook);
            self.board_collection.insert(u.mv.to - 2, &rook);
        }
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
    let mut game = Game::from_fen(START_POS);

    loop {
        let move_gen = MoveGen::new(&game);

        let input = take_input();

        if input.starts_with("position") {
            let parts = input.split_ascii_whitespace().collect::<Vec<&str>>();

            if parts[1] == "startpos" {
                game = Game::from_fen(START_POS);
            } else if parts[1] == "fen" {
                // position fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
                // the fen is parts 2..=7

                let mut new_game = Game::from_fen(parts[2..=7].join(" ").as_str());
                let move_gen = MoveGen::new(&new_game);

                if parts.len() < 9 || parts[8] != "moves" {
                    game = new_game;
                    continue;
                }

                for mv in parts[9..parts.len()].iter() {
                    if mv.len() != 4 {
                        continue;
                    }
                }
            }
        }

        if input.starts_with("go perft") {
            let track = Instant::now();
            let n = input.split_whitespace().collect::<Vec<&str>>()[2]
                .parse::<u8>()
                .unwrap_or(0);
            println!(
                "\ntotal: {} ({}s)",
                count_positions_n_deep(n, &mut game, true),
                track.elapsed().as_secs_f32()
            )
        }

        if input == "d" {
            println!("{}", game.board_collection);
            println!("Fen: {}", game.into_fen());
        }
    }
}

// Perft function
fn count_positions_n_deep(n: u8, game: &mut Game, split: bool) -> u32 {
    if n == 0 {
        return 1;
    }

    let mut s = 0;
    let move_gen = MoveGen::new(game);
    let color = if game.white_turn {
        Color::White
    } else {
        Color::Black
    };

    let mut moves = move_gen.legal_moves(color);

    if split {
        moves.sort_by(|a, b| {
            let mut a_s = BC::encode_notation(a.from);
            a_s.extend(BC::encode_notation(a.to).chars());

            let mut b_s = BC::encode_notation(b.from);
            b_s.extend(BC::encode_notation(b.to).chars());

            a_s.cmp(&b_s)
        });
    }

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

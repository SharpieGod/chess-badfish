use crate::board::*;
use crate::consts::*;

pub struct MoveGen<'a> {
    pub game: &'a Game,
    pub bc: &'a BitBoardCollection,
    pub white_attacks: BitBoard,
    pub black_attacks: BitBoard,
    pub occupied: BitBoard,
    pub white_occ: BitBoard,
    pub black_occ: BitBoard,
}
// attack = threats/protections
// quiets = empty spaces that the piece can move to
// captures = attack & opposite_color
// protections = attack & same_color
// moves = quiets | captures
// lists need to be split up in quiets and captures, pawns have special double push for en_passant tracking
impl<'a> MoveGen<'a> {
    pub fn new(game: &'a Game) -> Self {
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

    pub fn filter_legal(pseudo_moves: Vec<Move>, game: &mut Game, color: Color) -> Vec<Move> {
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

    pub fn occupied_color(&self, color: Color) -> BitBoard {
        match color {
            Color::White => self.white_occ,
            Color::Black => self.black_occ,
        }
    }

    pub fn attacks_by(&self, color: Color) -> BitBoard {
        match color {
            Color::White => self.white_attacks,
            Color::Black => self.black_attacks,
        }
    }

    pub fn pseudo_legal_moves(&self, color: Color) -> Vec<Move> {
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

    pub fn piece_captures(&self, index: u8, piece: ChessPiece) -> BitBoard {
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

    pub fn piece_quiets(&self, index: u8, piece: ChessPiece) -> BitBoard {
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

    pub fn piece_moves(&self, index: u8, piece: ChessPiece) -> BitBoard {
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

    pub fn piece_moves_list(&self, index: u8, piece: ChessPiece) -> Vec<Move> {
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

    pub fn basic_moves_list(&self, index: u8, piece: ChessPiece) -> Vec<Move> {
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
    pub fn pawn_en_passant(&self, index: u8, color: Color) -> BitBoard {
        // En passant is a capture, but also a threat.

        let en_passant = self
            .game
            .en_passant_square
            .map(|ep| BitBoard(1 << ep))
            .unwrap_or(BitBoard(0));

        en_passant
    }
    pub fn pawn_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.pawn_attacks(index, color)
            & (self.pawn_en_passant(index, color) | self.occupied_color(!color))
    }
    pub fn pawn_single(&self, index: u8, color: Color) -> BitBoard {
        let forward = color.pawn_forward();
        let single = BitBoard(0b1 << (index as i16 + forward)) & !self.occupied;

        single
    }
    pub fn pawn_quiets(&self, index: u8, color: Color) -> BitBoard {
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

    pub fn pawn_moves(&self, index: u8, color: Color) -> BitBoard {
        self.pawn_quiets(index, color) | self.pawn_captures(index, color)
    }

    pub fn pawn_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
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

    pub fn knight_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.knight_attacks(index) & self.occupied_color(!color)
    }
    pub fn knight_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.knight_attacks(index) & !self.occupied
    }
    pub fn knight_moves(&self, index: u8, color: Color) -> BitBoard {
        self.knight_quiets(index, color) | self.knight_captures(index, color)
    }
    pub fn knight_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        self.basic_moves_list(
            index,
            ChessPiece {
                kind: PieceKind::Knight,
                color,
            },
        )
    }

    // -- King --
    pub fn king_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.king_attacks(index)
            & self.occupied_color(!color)
            & !(self.attacks_by(!color))
    }
    pub fn king_quiets(&self, index: u8, color: Color) -> BitBoard {
        // King cant move to an attacked square
        self.game.board_collection.king_attacks(index) & !(self.attacks_by(!color)) & !self.occupied
    }
    pub fn king_castling_moves(&self, index: u8, color: Color, king_side: bool) -> BitBoard {
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

    pub fn king_moves(&self, index: u8, color: Color) -> BitBoard {
        self.king_quiets(index, color)
            | self.king_captures(index, color)
            | self.king_castling_moves(index, color, true)
            | self.king_castling_moves(index, color, false)
    }

    pub fn king_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
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

    pub fn rook_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.rook_attacks(index) & self.occupied_color(!color)
    }
    pub fn rook_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.rook_attacks(index) & !self.occupied
    }
    pub fn rook_moves(&self, index: u8, color: Color) -> BitBoard {
        self.rook_quiets(index, color) | self.rook_captures(index, color)
    }
    pub fn rook_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
        self.basic_moves_list(
            index,
            ChessPiece {
                kind: PieceKind::Rook,
                color,
            },
        )
    }

    // -- Bishop --

    pub fn bishop_captures(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.bishop_attacks(index) & self.occupied_color(!color)
    }
    pub fn bishop_quiets(&self, index: u8, color: Color) -> BitBoard {
        self.game.board_collection.bishop_attacks(index) & !self.occupied
    }
    pub fn bishop_moves(&self, index: u8, color: Color) -> BitBoard {
        self.bishop_quiets(index, color) | self.bishop_captures(index, color)
    }
    pub fn bishop_moves_list(&self, index: u8, color: Color) -> Vec<Move> {
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

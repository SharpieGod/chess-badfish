use std::collections::HashMap;
use std::time::Instant;

use crate::movegen::*;
use crate::{START_POS, consts::*};
use crate::{board::*, tables::*};

use crate::board::BitBoardCollection as BC;

#[derive(Clone, Copy)]
enum TTFlag {
    Exact,
    LowerBound,
    UpperBound,
}

#[derive(Clone, Copy)]
pub struct TTEntry {
    hash: u64,
    depth: u8,
    score: i32,
    flag: TTFlag,
    best_move: Option<Move>,
}

pub struct Engine {
    pub game: Game,
    pub history: HashMap<u64, u8>,
    pub tt: Vec<Option<TTEntry>>,
    pub nodes: u64,
    pub game_history: HashMap<u64, u8>, // Order doesnt matter, can appear multiple times
    pub last_score: i32,
    pub stop: bool,
    pub cached_attacks: [BitBoard; 2],
    pub cache_hash: u64,
    pub killers: [[Option<Move>; 2]; 64],
    pub history_table: [[[i32; 64]; 64]; 2],
}

// Values from PeSTO

impl Engine {
    pub fn new() -> Self {
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

    pub fn smallest_attacker(
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

    pub fn see(&self, target_sq: u8, target_value: i32, from_sq: u8, attacker_value: i32) -> i32 {
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

    pub fn refresh_cache(&mut self) {
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

    pub fn hanging_penalty(
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

    pub fn king_safety(&self, color: Color) -> i32 {
        let bc = &self.game.board_collection;
        let king_sq = bc
            .get_board(&color.kind(PieceKind::King))
            .0
            .trailing_zeros() as u8;
        let (king_file, king_rank) = BC::decode_tile(king_sq);

        let mut around = BitBoard(0);
        let (file, rank) = BC::decode_tile(king_sq);
        for (df, dr) in KING_DIRECTIONS {
            let (nf, nr) = (file as i8 + df, rank as i8 + dr);
            if (0..8).contains(&nf) && (0..8).contains(&nr) {
                around.0 |= 1 << BC::encode_tile(nf as u8, nr as u8);
            }
        }

        let mut attack_units = 0i32;
        let mut attacker_count = 0;
        let enemy = !color;

        for kind in 1..5usize {
            let piece = enemy.kind(kind.try_into().unwrap());
            let mut bb = *bc.get_board(&piece);

            while let Some(sq) = bb.pop_lsb() {
                let attack = bc.piece_attacks(sq, &piece);
                let zone_attacks = attack.0 & around.0;

                if zone_attacks != 0 {
                    attacker_count += 1;
                    let num_attacked = zone_attacks.count_ones() as i32;
                    attack_units += ATTACK_WEIGHTS[kind] * num_attacked
                }
            }
        }

        let friendly_pawns = bc.get_board(&color.kind(PieceKind::Pawn)).0;
        let mut shield_bonus = 0i32;

        for df in -1..=1i8 {
            let f = king_file as i8 + df;
            if !(0..8).contains(&f) {
                continue;
            }

            let forward_rank = if color == Color::White {
                king_rank + 1
            } else {
                king_rank.wrapping_sub(1)
            };

            if forward_rank < 8 {
                let shield_sq = BC::encode_tile(f as u8, forward_rank);
                if friendly_pawns & (1 << shield_sq) != 0 {
                    shield_bonus += PS_FULL_COVER; // pawn directly shielding
                } else {
                    let far_rank = if color == Color::White {
                        king_rank + 2
                    } else {
                        king_rank.wrapping_sub(2)
                    };

                    if far_rank < 8 {
                        let far_sq = BC::encode_tile(f as u8, far_rank);
                        if friendly_pawns & (1 << far_sq) != 0 {
                            shield_bonus += PS_PART_COVER; // pawn shield but advanced
                        } else {
                            shield_bonus += PS_NO_COVER; // no pawn cover at all
                        }
                    }
                }
            }
        }

        let file_mask: u64 = 0x0101010101010101;
        let all_pawns = bc.get_board(&Color::White.kind(PieceKind::Pawn)).0
            | bc.get_board(&Color::Black.kind(PieceKind::Pawn)).0;
        let mut open_file_penalty = 0i32;

        for df in -1..=1i8 {
            let f = king_file as i8 + df;
            if !(0..8).contains(&f) {
                continue;
            }
            if all_pawns & (file_mask << f) == 0 {
                open_file_penalty += OPEN_FILE_PENALTY; // fully open file near king
            } else if friendly_pawns & (file_mask << f as u8) == 0 {
                open_file_penalty += SEMI_OPEN_FILE_PENALTY; // semi-open (no friendly pawn)
            }
        }
        let safety_table_index = (attack_units as usize).min(99);
        let attack_score = SAFETY_TABLE[safety_table_index];
        let attack_penalty = if attacker_count >= ATTACKER_THRESHOLD {
            attack_score
        } else {
            0
        };

        shield_bonus - attack_penalty - open_file_penalty
    }

    pub fn static_eval(&self) -> i32 {
        let color = if self.game.white_turn {
            Color::White
        } else {
            Color::Black
        };
        let bc = &self.game.board_collection;

        let mut mg = [0; 2];
        let mut eg = [0; 2];
        let mut game_phase = 0i32;

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

        let king_safety = (self.king_safety(color) - self.king_safety(!color)) * mg_phase / 24;

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
            (our_rights - their_rights) * CASTLING_RIGHTS_BONUS
        };

        let file_mask: u64 = 0x0101010101010101;
        let all_pawns = friendly_pawns.0 | enemy_pawns.0;

        let mut rook_bonus = 0i32;

        // Friendly rooks
        let mut friendly_rooks = *bc.get_board(&color.kind(PieceKind::Rook));
        while let Some(sq) = friendly_rooks.pop_lsb() {
            let (file, rank) = BC::decode_tile(sq);
            let file_bb = file_mask << file;

            if file_bb & all_pawns == 0 {
                rook_bonus += ROOK_OPEN_FILE_BONUS; // open file
            } else if file_bb & friendly_pawns.0 == 0 {
                rook_bonus += ROOK_SEMI_OPEN_FILE_BONUS; // semi-open
            }

            // 7th rank bonus
            let seventh = if color == Color::White { 6 } else { 1 };

            if rank == seventh {
                rook_bonus += ROOK_SEVENTH_RANK_BONUS;
            }
        }

        // Enemy rooks
        let mut enemy_rooks = *bc.get_board(&(!color).kind(PieceKind::Rook));
        while let Some(sq) = enemy_rooks.pop_lsb() {
            let (file, rank) = BC::decode_tile(sq);
            let file_bb = file_mask << file;

            if file_bb & all_pawns == 0 {
                rook_bonus -= ROOK_OPEN_FILE_BONUS;
            } else if file_bb & enemy_pawns.0 == 0 {
                rook_bonus -= ROOK_SEMI_OPEN_FILE_BONUS;
            }

            let seventh = if color == Color::White { 1 } else { 6 };
            if rank == seventh {
                rook_bonus -= ROOK_SEVENTH_RANK_BONUS;
            }
        }

        (mg_score * mg_phase + eg_score * eg_phase) / 24
            + king_safety
            + bishops
            + castling_rights_bonus
            + pawns
            + self.mobility_bonus()
            + rook_bonus
    }

    pub fn mobility_bonus(&self) -> i32 {
        let bc = &self.game.board_collection;
        let color = if self.game.white_turn {
            Color::White
        } else {
            Color::Black
        };

        let mut score = 0i32;

        let weights = MOBILITY_BONUS; // knight mobility matters most per-square

        for (kind_idx, &weight) in [1usize, 2, 3, 4].iter().zip(weights.iter()) {
            let kind: PieceKind = (*kind_idx).try_into().unwrap();

            // Friendly mobility
            let piece = color.kind(kind);
            let mut bb = *bc.get_board(&piece);
            let friendly_occ = bc.occupied_color(color);
            while let Some(sq) = bb.pop_lsb() {
                let attacks = bc.piece_attacks(sq, &piece);
                let moves = (attacks & !friendly_occ).0.count_ones() as i32;
                score += weight * moves;
            }

            // Enemy mobility (subtract)
            let enemy_piece = (!color).kind(kind);
            let mut bb = *bc.get_board(&enemy_piece);
            let enemy_occ = bc.occupied_color(!color);
            while let Some(sq) = bb.pop_lsb() {
                let attacks = bc.piece_attacks(sq, &enemy_piece);
                let moves = (attacks & !enemy_occ).0.count_ones() as i32;
                score -= weight * moves;
            }
        }

        score
    }

    pub fn pawn_bonus(&self, friendly_pawns: BitBoard, enemy_pawns: BitBoard, color: Color) -> i32 {
        // Passed pawns detection
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
                bonus -= ISOLATED_PAWN_PENALTY;
            }
        }

        for file in 0..8 {
            let pawns_on_file = (file_mask << file & friendly_pawns.0).count_ones();
            if pawns_on_file > 1 {
                bonus -= DOUBLED_PAWNS_PENALTY * (pawns_on_file - 1) as i32;
            }
        }

        bonus
    }

    pub fn bishop_bonus(&self, bishop_count: u8) -> i32 {
        if bishop_count < 2 { 0 } else { BISHOP_BONUS }
    }

    pub fn search(&mut self, max_depth: u8, time_ms: u64) -> Option<Move> {
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

    pub fn search_at_depth(
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

    pub fn negamax(
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

    pub fn move_score(&self, m: &Move, ply: u8, color: Color) -> i32 {
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

    pub fn see_for_move(&self, m: &Move) -> i32 {
        if m.flags.contains(MoveFlags::EN_PASSANT) {
            return 0;
        }

        let bc = &self.game.board_collection;
        let target = bc.piece_at_index(m.to).unwrap();
        let attacker = bc.piece_at_index(m.from).unwrap();
        self.see(m.to, target.kind.value(), m.from, attacker.kind.value())
    }

    pub fn quiescence(&mut self, mut alpha: i32, beta: i32) -> i32 {
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

    pub fn debug_eval(&mut self) {
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

        let king_safety = (self.king_safety(color) - self.king_safety(!color)) * mg_phase / 24;
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

    pub fn material_score(&self, color: Color) -> i32 {
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

pub fn score_to_tt(score: i32, ply: u8) -> i32 {
    if score > 900_000 {
        score + ply as i32
    } else if score < -900_000 {
        score - ply as i32
    } else {
        score
    }
}

pub fn score_from_tt(score: i32, ply: u8) -> i32 {
    if score > 900_000 {
        score - ply as i32
    } else if score < -900_000 {
        score + ply as i32
    } else {
        score
    }
}

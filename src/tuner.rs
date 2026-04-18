use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};

use crate::{
    board::{Color, Game, PieceKind},
    engine::Engine,
    tables::*,
};

#[derive(Default, Clone)]
pub struct Trace {
    pub piece_values: [[i8; 5]; 2], // P N B R Q

    pub passed_pawns: [[i8; 8]; 2], // by rank
    pub isolated_pawns: [i8; 2],
    pub doubled_pawns: [i8; 2],

    pub ps_no_cover: [i8; 2],
    pub ps_part_cover: [i8; 2],
    pub ps_full_cover: [i8; 2],
    pub open_file_penalty: [i8; 2],
    pub semi_open_file_penalty: [i8; 2],

    pub bishop_pair: [i8; 2],
    pub castling_rights: [i8; 2],
    pub rook_open_file: [i8; 2],
    pub rook_semi_open_file: [i8; 2],
    pub rook_seventh_rank: [i8; 2],

    pub mobility: [[i8; 4]; 2], // N B R Q
    pub attack_units: [i8; 2],  // raw weighted sum before table lookup
    pub attacker_count: [i8; 2],
    pub phase: i32,
}

pub struct AdamState {
    pub params: Vec<f64>, // shadow float params, round to i32 for eval
    pub m: Vec<f64>,      // first moment
    pub v: Vec<f64>,      // second moment
    pub t: usize,         // step counter

    pub lr: f64,    // learning rate, start with 1.0
    pub beta1: f64, // 0.9
    pub beta2: f64, // 0.999
    pub eps: f64,   // 1e-8
}

impl AdamState {
    pub fn new(initial_params: Vec<f64>) -> Self {
        let n = initial_params.len();
        Self {
            params: initial_params,
            m: vec![0.0; n],
            v: vec![0.0; n],
            t: 0,
            lr: 1.0,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
        }
    }

    pub fn step(&mut self, grad: &[f64]) {
        self.t += 1;
        let t = self.t as f64;
        let bc1 = 1.0 - self.beta1.powf(t);
        let bc2 = 1.0 - self.beta2.powf(t);

        for i in 0..self.params.len() {
            let g = grad[i];
            self.m[i] = self.beta1 * self.m[i] + (1.0 - self.beta1) * g;
            self.v[i] = self.beta2 * self.v[i] + (1.0 - self.beta2) * g * g;

            let m_hat = self.m[i] / bc1;
            let v_hat = self.v[i] / bc2;

            self.params[i] -= self.lr * m_hat / (v_hat.sqrt() + self.eps);
        }
    }
}

pub fn score_from_trace(trace: &Trace, white_turn: bool, game: &Game) -> i32 {
    let diff = |arr: [i8; 2]| (arr[0] - arr[1]) as i32;
    let diff_idx = |arr: [[i8; 8]; 2], i: usize| (arr[0][i] - arr[1][i]) as i32;
    let diff_mob = |arr: [[i8; 4]; 2], i: usize| (arr[0][i] - arr[1][i]) as i32;

    // PST + material
    let mut mg = [0i32; 2];
    let mut eg = [0i32; 2];
    let mut game_phase = 0i32;

    for sq in 0..64u8 {
        if let Some(piece) = game.board_collection.piece_at_index(sq) {
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

            let kind_idx = piece.kind as usize;
            mg[color_idx] += mg_val + MG_PIECE_VALUES[kind_idx];
            eg[color_idx] += eg_val + EG_PIECE_VALUES[kind_idx];
            game_phase += GAMEPHASE_INC[kind_idx];
        }
    }

    let phase = game_phase.min(24);
    let eg_phase = 24 - phase;
    let pst_score = ((mg[0] - mg[1]) * phase + (eg[0] - eg[1]) * eg_phase) / 24;

    // Scalar terms
    let passed = (0..8)
        .map(|r| diff_idx(trace.passed_pawns, r) * PASSED_PAWN_BONUS[r])
        .sum::<i32>();
    let isolated = diff(trace.isolated_pawns) * ISOLATED_PAWN_PENALTY;
    let doubled = diff(trace.doubled_pawns) * DOUBLED_PAWNS_PENALTY;
    let bishops = diff(trace.bishop_pair) * BISHOP_BONUS;
    let castling = diff(trace.castling_rights) * CASTLING_RIGHTS_BONUS;
    let rook_open = diff(trace.rook_open_file) * ROOK_OPEN_FILE_BONUS;
    let rook_semi = diff(trace.rook_semi_open_file) * ROOK_SEMI_OPEN_FILE_BONUS;
    let rook_seventh = diff(trace.rook_seventh_rank) * ROOK_SEVENTH_RANK_BONUS;
    let mobility = (0..4)
        .map(|i| diff_mob(trace.mobility, i) * MOBILITY_BONUS[i])
        .sum::<i32>();

    // King safety — per side then diff, mirroring static_eval exactly
    let king_safety = {
        let mut result = 0i32;
        for c in 0..2usize {
            let shield = trace.ps_full_cover[c] as i32 * PS_FULL_COVER
                + trace.ps_part_cover[c] as i32 * PS_PART_COVER
                + trace.ps_no_cover[c] as i32 * PS_NO_COVER;

            let open = trace.open_file_penalty[c] as i32 * OPEN_FILE_PENALTY
                + trace.semi_open_file_penalty[c] as i32 * SEMI_OPEN_FILE_PENALTY;

            let penalty = if trace.attacker_count[c] >= ATTACKER_THRESHOLD as i8 {
                let idx = (trace.attack_units[c] as i32).clamp(0, 99) as usize;
                SAFETY_TABLE[idx]
            } else {
                0
            };

            let side_score = shield - penalty - open;
            if c == 0 {
                result += side_score;
            } else {
                result -= side_score;
            }
        }

        result * phase / 24
    };

    let score = pst_score
        + passed
        + isolated
        + doubled
        + king_safety
        + bishops
        + castling
        + rook_open
        + rook_semi
        + rook_seventh
        + mobility;

    if white_turn { score } else { -score }
}

pub fn tune() {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open("positions.txt").unwrap();
    let reader = BufReader::new(file);
    let mut engine = Engine::new();

    let mut positions: Vec<(Trace, bool, f32)> = Vec::new();

    for line in reader.lines() {
        let line = line.unwrap();
        let mut parts = line.splitn(2, '|');
        let fen = parts.next().unwrap().trim();
        let outcome: f32 = parts.next().unwrap().trim().parse().unwrap();
        engine.game = Game::from_fen(fen);
        let trace = engine.trace_eval();
        positions.push((trace, engine.game.white_turn, outcome));
    }

    eprintln!("Loaded {} positions", positions.len());

    fn to_params(v: &[f64]) -> Vec<f64> {
        v.to_vec()
    }

    fn initial_params() -> Vec<f64> {
        let mut v = Vec::new();
        for x in PASSED_PAWN_BONUS {
            v.push(x as f64);
        }
        v.push(ISOLATED_PAWN_PENALTY as f64);
        v.push(DOUBLED_PAWNS_PENALTY as f64);
        v.push(BISHOP_BONUS as f64);
        v.push(CASTLING_RIGHTS_BONUS as f64);
        v.push(ROOK_OPEN_FILE_BONUS as f64);
        v.push(ROOK_SEMI_OPEN_FILE_BONUS as f64);
        v.push(ROOK_SEVENTH_RANK_BONUS as f64);
        for x in MOBILITY_BONUS {
            v.push(x as f64);
        }
        v.push(PS_FULL_COVER as f64);
        v.push(PS_PART_COVER as f64);
        v.push(PS_NO_COVER as f64);
        v.push(OPEN_FILE_PENALTY as f64);
        v.push(SEMI_OPEN_FILE_PENALTY as f64);
        for x in &MG_PIECE_VALUES[..5] {
            v.push(*x as f64);
        }
        for x in &EG_PIECE_VALUES[..5] {
            v.push(*x as f64);
        }
        v
    }

    fn score_with_params(trace: &Trace, white_turn: bool, v: &[f64]) -> f32 {
        let mut i = 0;
        let passed_pawn_bonus: [f64; 8] = v[i..i + 8].try_into().unwrap();
        i += 8;
        let isolated_pawn_penalty = v[i];
        i += 1;
        let doubled_pawns_penalty = v[i];
        i += 1;
        let bishop_bonus = v[i];
        i += 1;
        let castling_rights_bonus = v[i];
        i += 1;
        let rook_open_file_bonus = v[i];
        i += 1;
        let rook_semi_open_file = v[i];
        i += 1;
        let rook_seventh_rank = v[i];
        i += 1;
        let mobility_bonus: [f64; 4] = v[i..i + 4].try_into().unwrap();
        i += 4;
        let ps_full_cover = v[i];
        i += 1;
        let ps_part_cover = v[i];
        i += 1;
        let ps_no_cover = v[i];
        i += 1;
        let open_file_penalty = v[i];
        i += 1;
        let semi_open_file_penalty = v[i];
        i += 1;
        let mg_piece: [f64; 5] = v[i..i + 5].try_into().unwrap();
        i += 5;
        let eg_piece: [f64; 5] = v[i..i + 5].try_into().unwrap();

        let diff = |arr: [i8; 2]| (arr[0] - arr[1]) as f64;
        let diffi = |arr: [[i8; 8]; 2], j: usize| (arr[0][j] - arr[1][j]) as f64;
        let diffm = |arr: [[i8; 4]; 2], j: usize| (arr[0][j] - arr[1][j]) as f64;

        let phase = trace.phase.min(24) as f64;
        let eg_phase = 24.0 - phase;

        // Material (piece values only, no PST — PST is fixed)
        let mg_mat: f64 = (0..5)
            .map(|j| (trace.piece_values[0][j] - trace.piece_values[1][j]) as f64 * mg_piece[j])
            .sum();
        let eg_mat: f64 = (0..5)
            .map(|j| (trace.piece_values[0][j] - trace.piece_values[1][j]) as f64 * eg_piece[j])
            .sum();
        let material = (mg_mat * phase + eg_mat * eg_phase) / 24.0;

        let passed: f64 = (0..8)
            .map(|r| diffi(trace.passed_pawns, r) * passed_pawn_bonus[r])
            .sum();
        let isolated = diff(trace.isolated_pawns) * isolated_pawn_penalty;
        let doubled = diff(trace.doubled_pawns) * doubled_pawns_penalty;
        let bishops = diff(trace.bishop_pair) * bishop_bonus;
        let castling = diff(trace.castling_rights) * castling_rights_bonus;
        let rook_open = diff(trace.rook_open_file) * rook_open_file_bonus;
        let rook_semi = diff(trace.rook_semi_open_file) * rook_semi_open_file;
        let rook_7th = diff(trace.rook_seventh_rank) * rook_seventh_rank;
        let mobility: f64 = (0..4)
            .map(|j| diffm(trace.mobility, j) * mobility_bonus[j])
            .sum();

        let king_safety = {
            let mut result = 0.0f64;
            for c in 0..2usize {
                let shield = trace.ps_full_cover[c] as f64 * ps_full_cover
                    + trace.ps_part_cover[c] as f64 * ps_part_cover
                    + trace.ps_no_cover[c] as f64 * ps_no_cover;
                let open = trace.open_file_penalty[c] as f64 * open_file_penalty
                    + trace.semi_open_file_penalty[c] as f64 * semi_open_file_penalty;
                let penalty = if trace.attacker_count[c] >= ATTACKER_THRESHOLD as i8 {
                    let idx = (trace.attack_units[c] as i32).clamp(0, 99) as usize;
                    SAFETY_TABLE[idx] as f64
                } else {
                    0.0
                };
                let side = shield - penalty - open;
                if c == 0 {
                    result += side;
                } else {
                    result -= side;
                }
            }
            result * phase / 24.0
        };

        let score = material
            + passed
            + isolated
            + doubled
            + bishops
            + castling
            + rook_open
            + rook_semi
            + rook_7th
            + mobility
            + king_safety;

        if white_turn {
            score as f32
        } else {
            -score as f32
        }
    }

    fn sigmoid(s: f32, k: f32) -> f32 {
        1.0 / (1.0 + (-k * s).exp())
    }

    fn total_loss(positions: &[(Trace, bool, f32)], params: &[f64], k: f32) -> f64 {
        positions
            .par_iter()
            .map(|(trace, wt, outcome)| {
                let s = score_with_params(trace, *wt, params);
                let p = sigmoid(s, k);
                let d = p - outcome;

                (d * d) as f64
            })
            .sum::<f64>()
            / positions.len() as f64
    }

    eprintln!("Finding K...");
    let initial = initial_params();
    let (mut lo, mut hi) = (0.001f32, 0.01f32);
    for _ in 0..50 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if total_loss(&positions, &initial, m1) < total_loss(&positions, &initial, m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let k = (lo + hi) / 2.0;
    eprintln!("K = {:.6}", k);

    let mut adam = AdamState::new(initial);
    let eps = 1.0f64;
    let n_params = adam.params.len();

    for epoch in 0..2_000 {
        // Compute numerical gradient in parallel across params
        let base_loss = total_loss(&positions, &adam.params, k);

        let grads: Vec<f64> = (0..n_params)
            .into_par_iter()
            .map(|i| {
                let mut v = adam.params.clone();
                v[i] += eps;
                let l_plus = total_loss(&positions, &v, k);
                (l_plus - base_loss) / eps
            })
            .collect();

        adam.step(&grads);

        if epoch % 100 == 0 {
            eprintln!("epoch={:5} loss={:.6}", epoch, base_loss);
        }
    }

    // ------------------------------------------------------------------ //
    //  Print final params                                                  //
    // ------------------------------------------------------------------ //
    let v = &adam.params;
    let mut i = 0;
    eprintln!("\n=== Tuned Parameters ===");
    eprintln!(
        "PASSED_PAWN_BONUS: {:?}",
        v[i..i + 8].iter().map(|x| *x as i32).collect::<Vec<_>>()
    );
    i += 8;
    eprintln!("ISOLATED_PAWN_PENALTY: {}", v[i] as i32);
    i += 1;
    eprintln!("DOUBLED_PAWNS_PENALTY: {}", v[i] as i32);
    i += 1;
    eprintln!("BISHOP_BONUS: {}", v[i] as i32);
    i += 1;
    eprintln!("CASTLING_RIGHTS_BONUS: {}", v[i] as i32);
    i += 1;
    eprintln!("ROOK_OPEN_FILE_BONUS: {}", v[i] as i32);
    i += 1;
    eprintln!("ROOK_SEMI_OPEN_FILE_BONUS: {}", v[i] as i32);
    i += 1;
    eprintln!("ROOK_SEVENTH_RANK_BONUS: {}", v[i] as i32);
    i += 1;
    eprintln!(
        "MOBILITY_BONUS: {:?}",
        v[i..i + 4].iter().map(|x| *x as i32).collect::<Vec<_>>()
    );
    i += 4;
    eprintln!("PS_FULL_COVER: {}", v[i] as i32);
    i += 1;
    eprintln!("PS_PART_COVER: {}", v[i] as i32);
    i += 1;
    eprintln!("PS_NO_COVER: {}", v[i] as i32);
    i += 1;
    eprintln!("OPEN_FILE_PENALTY: {}", v[i] as i32);
    i += 1;
    eprintln!("SEMI_OPEN_FILE_PENALTY: {}", v[i] as i32);
    i += 1;
    eprintln!(
        "MG_PIECE_VALUES: {:?}",
        v[i..i + 5].iter().map(|x| *x as i32).collect::<Vec<_>>()
    );
    i += 5;
    eprintln!(
        "EG_PIECE_VALUES: {:?}",
        v[i..i + 5].iter().map(|x| *x as i32).collect::<Vec<_>>()
    );
}

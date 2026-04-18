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

    pub phase: i32, // for tapering, not a tunable param
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

// Castling masks, king side and queen side, black and white
pub const WHITE_K_EMPTY: u64 = (1 << 5) | (1 << 6);
pub const WHITE_K_SAFE: u64 = (1 << 4) | (1 << 5) | (1 << 6);
pub const WHITE_Q_EMPTY: u64 = (1 << 1) | (1 << 2) | (1 << 3);
pub const WHITE_Q_SAFE: u64 = (1 << 2) | (1 << 3) | (1 << 4);

pub const BLACK_K_EMPTY: u64 = (1 << 61) | (1 << 62);
pub const BLACK_K_SAFE: u64 = (1 << 60) | (1 << 61) | (1 << 62);
pub const BLACK_Q_EMPTY: u64 = (1 << 57) | (1 << 58) | (1 << 59);
pub const BLACK_Q_SAFE: u64 = (1 << 58) | (1 << 59) | (1 << 60);

pub const KING_DIRECTIONS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

pub const ROOK_DIRECTIONS: [(i8, i8); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
pub const BISHOP_DIRECTIONS: [(i8, i8); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

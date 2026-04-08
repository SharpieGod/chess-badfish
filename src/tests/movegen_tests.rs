// tests/movegen_tests.rs

use crate::*;

fn empty_game() -> Game {
    Game {
        board_collection: BitBoardCollection::new(),
        en_passant_square: None,
    }
}

fn game_from_fen(fen: &str) -> Game {
    Game {
        board_collection: BitBoardCollection::from_fen(fen),
        en_passant_square: None,
    }
}

#[test]
fn knight_center_moves() {
    let mut game = empty_game();
    let knight = ChessPiece {
        kind: PieceKind::Knight,
        color: Color::White,
    };
    game.board_collection.insert(BC::encode_tile(4, 4), &knight);

    let mg = MoveGen::new(&game);
    let moves = mg.knight_attacks(BC::encode_tile(4, 4), Color::White);

    assert_eq!(moves.0.count_ones(), 8);
}

#[test]
fn knight_edge_moves() {
    let mut game = empty_game();
    let knight = ChessPiece {
        kind: PieceKind::Knight,
        color: Color::White,
    };
    game.board_collection.insert(BC::encode_tile(0, 0), &knight);

    let mg = MoveGen::new(&game);
    let moves = mg.knight_attacks(BC::encode_tile(0, 0), Color::White);

    assert_eq!(moves.0.count_ones(), 2);
}

#[test]
fn rook_blocked_by_piece() {
    let mut game = empty_game();
    let rook = ChessPiece {
        kind: PieceKind::Rook,
        color: Color::White,
    };
    let blocker = ChessPiece {
        kind: PieceKind::Pawn,
        color: Color::White,
    };

    game.board_collection.insert(BC::encode_tile(4, 4), &rook);
    game.board_collection
        .insert(BC::encode_tile(4, 6), &blocker);

    let mg = MoveGen::new(&game);
    let attacks = mg.rook_attacks(BC::encode_tile(4, 4), Color::White);

    assert!(!attacks.contains(BC::encode_tile(4, 7))); // should be blocked
}

#[test]
fn bishop_blocked_by_enemy() {
    let mut game = empty_game();
    let bishop = ChessPiece {
        kind: PieceKind::Bishop,
        color: Color::White,
    };
    let enemy = ChessPiece {
        kind: PieceKind::Pawn,
        color: Color::Black,
    };

    game.board_collection.insert(BC::encode_tile(2, 2), &bishop);
    game.board_collection.insert(BC::encode_tile(4, 4), &enemy);

    let mg = MoveGen::new(&game);
    let attacks = mg.bishop_attacks(BC::encode_tile(2, 2), Color::White);

    assert!(attacks.contains(BC::encode_tile(4, 4))); // can capture
    assert!(!attacks.contains(BC::encode_tile(5, 5))); // cannot go past
}

#[test]
fn pawn_attacks_correct() {
    let mut game = empty_game();
    let pawn = ChessPiece {
        kind: PieceKind::Pawn,
        color: Color::White,
    };

    game.board_collection.insert(BC::encode_tile(4, 4), &pawn);

    let mg = MoveGen::new(&game);
    let attacks = mg.pawn_attacks(BC::encode_tile(4, 4), Color::White);

    assert!(attacks.contains(BC::encode_tile(3, 5)));
    assert!(attacks.contains(BC::encode_tile(5, 5)));
}

#[test]
fn pawn_double_push_blocked() {
    let mut game = empty_game();
    let pawn = ChessPiece {
        kind: PieceKind::Pawn,
        color: Color::White,
    };
    let blocker = ChessPiece {
        kind: PieceKind::Pawn,
        color: Color::Black,
    };

    game.board_collection.insert(BC::encode_tile(4, 1), &pawn);
    game.board_collection
        .insert(BC::encode_tile(4, 2), &blocker);

    let mg = MoveGen::new(&game);
    let moves = mg.pawn_quiets(BC::encode_tile(4, 1), Color::White);

    assert_eq!(moves.0, 0); // cannot move at all
}

#[test]
fn king_cannot_move_into_check() {
    let mut game = empty_game();
    let king = ChessPiece {
        kind: PieceKind::King,
        color: Color::White,
    };
    let rook = ChessPiece {
        kind: PieceKind::Rook,
        color: Color::Black,
    };

    game.board_collection.insert(BC::encode_tile(4, 4), &king);
    game.board_collection.insert(BC::encode_tile(4, 7), &rook);

    let mg = MoveGen::new(&game);
    let moves = mg.king_moves(BC::encode_tile(4, 4), Color::White);

    assert!(!moves.contains(BC::encode_tile(4, 5))); // attacked by rook
}

#[test]
fn all_attacked_by_basic() {
    let mut game = empty_game();
    let rook = ChessPiece {
        kind: PieceKind::Rook,
        color: Color::White,
    };

    game.board_collection.insert(BC::encode_tile(0, 0), &rook);

    let mg = MoveGen::new(&game);
    let attacks = mg.all_attacked_by(Color::White);

    assert!(attacks.contains(BC::encode_tile(0, 7)));
    assert!(attacks.contains(BC::encode_tile(7, 0)));
}

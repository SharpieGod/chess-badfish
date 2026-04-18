use std::fs;

use crate::{
    START_POS,
    board::{Color, Game},
    movegen::MoveGen,
};

pub fn parse_pgn() {
    let raw = fs::read_to_string("13kgames.pgn")
        .expect("Error reading games")
        .to_string();

    let mut i = 0;
    let chars: Vec<char> = raw.chars().collect();

    while i < raw.len() {
        let mut r = i;
        let mut in_squares = chars[r] == '[';

        while r < raw.len() {
            let c = chars[r];
            if c == '[' {
                in_squares = true;
            }

            if c == ']' {
                in_squares = false;
            }

            if c == '1' && !in_squares {
                break;
            }

            r += 1;
        }

        // r is at block start

        let mut l = r;

        while l < raw.len() && chars[l] != '[' {
            l += 1
        }

        let mut cleaned = String::new();
        let mut in_comment = false;

        for c in raw.chars().skip(r).take(l - r) {
            if c == '{' {
                in_comment = true;
                continue;
            }
            if c == '}' {
                in_comment = false;
                continue;
            }
            if !in_comment {
                cleaned.push(c);
            }
        }

        let mut tokens: Vec<&str> = cleaned
            .split_whitespace()
            .filter(|s| !s.contains('.'))
            .collect();

        let turnout = match tokens.pop().unwrap() {
            "1-0" => 1f32,
            "0-1" => 0f32,
            "1/2-1/2" => 0.5,
            _ => -9999f32,
        };

        let mut game = Game::from_fen(START_POS);
        let mut seen = 0;

        for (idx, t) in tokens.iter().enumerate() {
            let mv = match game.from_san(*t) {
                Some(mv) => mv,
                None => break,
            };

            game.make_move(&mv);

            let color = if game.white_turn {
                Color::White
            } else {
                Color::Black
            };

            // skip positions in check
            if t.contains('+') || t.contains('#') || game.board_collection.is_in_check(color) {
                continue;
            }

            seen += 1;

            if seen > 10 && seen % 3 == 0 {
                let outcome = if game.white_turn {
                    turnout
                } else {
                    1.0 - turnout
                };

                println!("{} | {}", game.into_fen(), outcome);
            }
        }

        i = l;
    }
}

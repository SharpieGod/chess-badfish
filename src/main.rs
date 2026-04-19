mod board;
mod consts;
mod engine;
mod movegen;
mod parse_pgn;
mod stockfish;
mod tables;
mod tuner;

use board::*;
use engine::*;
use movegen::*;
use parse_pgn::*;
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::sync::atomic::Ordering;
use std::thread;
use std::{collections::HashMap, i32, io, mem, sync::OnceLock, time::Instant};
use tables::*;
use tables::*;

use board::BitBoardCollection as BC;

use crate::tuner::tune;

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
    // parse_pgn();
    // tune();

    let mut engine = Engine::new();
    let mut search_thread: Option<thread::JoinHandle<()>> = None;
    let start = std::time::Instant::now();
    let input = engine.game.encode_for_nn();
    let mut dummy = 0i32;
    for _ in 0..100_000 {
        dummy += engine.nn.eval(&input);
    }
    println!("dummy: {}", dummy); // prevent optimization
    println!("100k evals: {}ms", start.elapsed().as_millis());

    loop {
        let input = take_input();

        if input == "quit" {
            std::process::exit(0);
        }

        if input == "uci" {
            println!("id name ViktorE");
            println!("id author DarkoS");
            println!("uciok");
        }

        if input == "isready" {
            println!("readyok");
        }

        if input.starts_with("setoption") {
            // ignore for now
        }

        if input == "stop" {
            engine.stop.store(true, Ordering::Relaxed);
            if let Some(t) = search_thread.take() {
                let _ = t.join();
            }
        }

        if input == "ucinewgame" {
            engine.stop.store(true, Ordering::Relaxed);
            if let Some(t) = search_thread.take() {
                let _ = t.join();
            }
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
            engine.stop.store(true, Ordering::Relaxed);
            if let Some(t) = search_thread.take() {
                let _ = t.join();
            }
            engine.stop.store(false, Ordering::Relaxed);

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

            let mut search_engine = engine.clone();

            search_thread = Some(thread::spawn(move || {
                if let Some(mv) = search_engine.search(depth, time_ms) {
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
            }));
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

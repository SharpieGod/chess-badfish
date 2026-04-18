// Also claude, the version I wrote was bad and slow, threading in rust is kinda scary too

use std::{
    cell::RefCell,
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

use rayon::prelude::*;

use crate::{
    START_POS,
    board::{Color, Game},
    stockfish::Stockfish,
};

const TARGET: usize = 1_500_000;
const MIN_ELO: u32 = 1900;
const MIN_TC_SECONDS: i32 = 300;
const SKIP_FIRST_N: usize = 10;
const SAMPLE_EVERY: usize = 12;

fn strip_line_comments(line: &str, in_comment: &mut bool) -> String {
    let mut out = String::new();
    for c in line.chars() {
        match c {
            '{' => *in_comment = true,
            '}' => *in_comment = false,
            _ if !*in_comment => out.push(c),
            _ => {}
        }
    }
    out
}

fn parse_tag_value(s: &str) -> Option<&str> {
    let start = s.find('"')? + 1;
    let end = s.rfind('"')?;
    if start < end {
        Some(&s[start..end])
    } else {
        None
    }
}

fn process_game(
    header: &str,
    moves_block: &str,
    positions: &mut Vec<(String, f32)>,
    seen_fens: &mut HashSet<String>,
) {
    if positions.len() >= TARGET {
        return;
    }

    let mut w_elo = 0u32;
    let mut b_elo = 0u32;
    let mut time_control: Option<String> = None;

    for line in header.lines() {
        let line = line.trim();
        if line.starts_with("[WhiteElo") {
            w_elo = parse_tag_value(line)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        } else if line.starts_with("[BlackElo") {
            b_elo = parse_tag_value(line)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        } else if line.starts_with("[TimeControl") {
            time_control = parse_tag_value(line).map(|s| s.to_string());
        }
    }

    if w_elo < MIN_ELO || b_elo < MIN_ELO {
        return;
    }

    if let Some(tc) = &time_control {
        let base = tc.split('+').next().unwrap_or("0");
        let seconds: i32 = base.parse().unwrap_or(0);
        if seconds < MIN_TC_SECONDS {
            return;
        }
    }

    let mut tokens: Vec<&str> = moves_block
        .split_whitespace()
        .filter(|s| !s.contains('.'))
        .collect();

    let result_token = match tokens.last().copied() {
        Some(t @ ("1-0" | "0-1" | "1/2-1/2")) => t,
        _ => return,
    };
    tokens.pop();

    let turnout: f32 = match result_token {
        "1-0" => 1.0,
        "0-1" => 0.0,
        "1/2-1/2" => 0.5,
        _ => return,
    };

    let mut game = Game::from_fen(START_POS);
    let mut seen = 0usize;

    for t in &tokens {
        if positions.len() >= TARGET {
            break;
        }

        let mv = match game.from_san(t) {
            Some(mv) => mv,
            None => break,
        };

        game.make_move(&mv);

        let color = if game.white_turn {
            Color::White
        } else {
            Color::Black
        };

        if t.contains('+') || t.contains('#') || game.board_collection.is_in_check(color) {
            continue;
        }

        seen += 1;

        if seen > SKIP_FIRST_N && seen % SAMPLE_EVERY == 0 {
            let outcome = if game.white_turn {
                turnout
            } else {
                1.0 - turnout
            };
            let fen = game.into_fen();
            if seen_fens.insert(fen.clone()) {
                positions.push((fen, outcome));
            }
        }
    }
}

pub fn parse_pgn() {
    // ------------------------------------------------------------------ //
    //  Phase 1: stream PGN line-by-line, collect up to TARGET positions   //
    // ------------------------------------------------------------------ //
    eprintln!("Phase 1: collecting positions from PGN...");

    let file = BufReader::new(File::open("lichess.pgn").expect("Cannot open lichess.pgn"));

    let mut positions: Vec<(String, f32)> = Vec::with_capacity(TARGET);
    let mut seen_fens: HashSet<String> = HashSet::with_capacity(TARGET);

    let mut current_header = String::new();
    let mut current_moves = String::new();
    let mut in_moves = false;
    let mut in_comment = false;
    let mut games_seen = 0usize;

    for raw_line in file.lines() {
        if positions.len() >= TARGET {
            break;
        }

        let raw_line = raw_line.expect("Error reading line");
        let line = strip_line_comments(&raw_line, &mut in_comment);
        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            if in_moves {
                games_seen += 1;
                if games_seen % 100_000 == 0 {
                    eprintln!(
                        "  games processed: {}  positions collected: {}",
                        games_seen,
                        positions.len()
                    );
                }
                process_game(
                    &current_header,
                    &current_moves,
                    &mut positions,
                    &mut seen_fens,
                );
                current_header.clear();
                current_moves.clear();
                in_moves = false;
            }
            current_header.push_str(&line);
            current_header.push('\n');
        } else if trimmed.is_empty() {
            // blank line — ignore
        } else {
            in_moves = true;
            current_moves.push_str(&line);
            current_moves.push('\n');
        }
    }

    // flush last game
    if !current_moves.is_empty() {
        process_game(
            &current_header,
            &current_moves,
            &mut positions,
            &mut seen_fens,
        );
    }

    drop(seen_fens); // free memory before SF phase

    eprintln!(
        "Phase 1 done: {} positions collected from {} games",
        positions.len(),
        games_seen
    );

    // ------------------------------------------------------------------ //
    //  Phase 2: annotate with Stockfish in parallel, write to file        //
    // ------------------------------------------------------------------ //
    eprintln!("Phase 2: annotating with Stockfish...");

    let total = positions.len();
    let done = Arc::new(AtomicUsize::new(0));

    let (tx, rx) = mpsc::channel::<String>();

    // dedicated writer thread so SF threads never block on I/O
    let writer_thread = std::thread::spawn(move || {
        let mut out =
            BufWriter::new(File::create("positions.txt").expect("Cannot create positions.txt"));
        for line in rx {
            writeln!(out, "{}", line).unwrap();
        }
        out.flush().unwrap();
    });

    positions
        .par_iter()
        .for_each_with((tx, done.clone()), |(tx, done), (fen, outcome)| {
            thread_local! {
                static SF: RefCell<Stockfish> = RefCell::new(Stockfish::new());
            }

            SF.with(|sf| {
                if let Some(cp) = sf.borrow_mut().eval(fen) {
                    let sf_sigmoid = 1.0 / (1.0 + (-cp as f32 / 400.0).exp());
                    let blended = 0.5 * sf_sigmoid + 0.5 * outcome;
                    tx.send(format!("{} | {:.4}", fen, blended)).unwrap();
                }

                let n = done.fetch_add(1, Ordering::Relaxed);
                if n % 10_000 == 0 {
                    eprintln!(
                        "  annotated: {}/{} ({:.1}%)",
                        n,
                        total,
                        n as f32 / total as f32 * 100.0
                    );
                }
            });
        });

    writer_thread.join().unwrap();

    eprintln!("Done! positions.txt written.");
}

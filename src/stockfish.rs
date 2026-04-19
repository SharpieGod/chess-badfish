// Claude generated for stockfish integration for texel tuning data generation
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

pub struct Stockfish {
    process: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Stockfish {
    pub fn new() -> Self {
        let mut process = Command::new("stockfish")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start Stockfish");

        let stdin = process.stdin.take().unwrap();
        let stdout = BufReader::new(process.stdout.take().unwrap());

        let mut sf = Self {
            process,
            stdin,
            stdout,
        };
        sf.send("uci");
        sf.wait_for("uciok");
        sf.send("setoption name Hash value 128");
        sf.send("isready");
        sf.wait_for("readyok");
        sf
    }

    fn send(&mut self, cmd: &str) {
        writeln!(self.stdin, "{}", cmd).unwrap();
    }

    fn wait_for(&mut self, target: &str) {
        let mut line = String::new();
        loop {
            line.clear();
            self.stdout.read_line(&mut line).unwrap();
            if line.contains(target) {
                break;
            }
        }
    }

    /// Returns centipawn score from white's perspective, capped at +/- 1000
    pub fn eval(&mut self, fen: &str) -> Option<i32> {
        self.send(&format!("position fen {}", fen));
        self.send(&format!("go depth 5"));

        let mut score: Option<i32> = None;
        let mut line = String::new();

        loop {
            line.clear();
            self.stdout.read_line(&mut line).unwrap();

            // parse "info ... score cp 34 ..." or "score mate N"
            if line.starts_with("info") && line.contains("score") {
                if let Some(cp) = parse_score(&line) {
                    score = Some(cp);
                }
            }

            if line.starts_with("bestmove") {
                break;
            }
        }

        score
    }
}

fn parse_score(info: &str) -> Option<i32> {
    let tokens: Vec<&str> = info.split_whitespace().collect();
    let idx = tokens.iter().position(|&t| t == "score")?;

    match tokens.get(idx + 1)? {
        &"cp" => {
            let cp: i32 = tokens.get(idx + 2)?.parse().ok()?;
            Some(cp.clamp(-1000, 1000))
        }
        &"mate" => {
            let n: i32 = tokens.get(idx + 2)?.parse().ok()?;
            Some(if n > 0 { 1000 } else { -1000 })
        }
        _ => None,
    }
}

impl Drop for Stockfish {
    fn drop(&mut self) {
        let _ = self.send("quit");
    }
}

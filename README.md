# ViktorE

A UCI chess engine written in Rust, with a small neural network evaluation exported from PyTorch to ONNX. Inspired by Sebastian Lague's chess programming videos on YouTube.

Strength is around 1650 Elo based on self-play and lichess games.

## How To Use
[Hosted on lichess](https://lichess.org/@/ViktorEChessBot)

You have to create an account to play, then press `challenge` on my bot's profile, and select a `time control` (you need to specify a time control, it **cant handle infinite time**).

## Features

### Search

- Iterative deepening with aspiration windows
- Alpha-beta search (negamax) with transposition table
- Late move reductions (LMR) and late move pruning (LMP)
- Null move pruning with verification at high depth
- Reverse futility pruning
- Frontier futility pruning
- Quiescence search with SEE-based capture pruning
- Killer moves and history heuristic with gravity
- Check extensions
- Repetition and fifty-move rule detection

### Evaluation

Dual evaluation setup. The neural network is used during search; the hand-crafted evaluation is kept for tuning and debugging.

- **Neural network**: 782-input MLP trained on self-play and Stockfish-labeled positions, exported to ONNX and run via the `ort` crate
- **Hand-crafted evaluation**: PeSTO-style piece-square tables, mobility, pawn structure (passed, isolated, doubled), king safety (attack units, pawn shield, open files), rook on open/seventh, bishop pair, castling rights. Weights are Texel-tuned.

## Layout

```
src/                      engine source (board, move generation, search, eval)
neural-network/           training code (PyTorch), model files, exporter
saved-brains/             archived network versions (v0–v13)
scripts/                  helpers for elo estimation, data generation, batch testing
openings.pgn              opening book positions
training-data.txt         labeled positions for NN training
lichess-bot/              lichess-bot integration for online play
```

## Running Locally

After cloning the repo, create a symbolic link at `/opt/chess` to `/path/to/repo/neural-network`.
So that the binary can find the neural network weights.
Requires Rust (edition 2024, so 1.85+) and ONNX Runtime.

You will have to generate your own dataset and weights, becasue theyre too large to host anywhere (12 GB),
the script to generate datasets is in the `/src` directory (download a large game dataset from lichess and name it `lichess.pgn`),
make sure the generate positions are in `/training_data.txt`
the script to train the network is in the `/neural-network` directory.

```
cargo build --release
```

The binary ends up at `target/release/chess-badfish`.

## Running

As a UCI engine:

```
./target/release/chess-badfish
```

Then send UCI commands on stdin (`uci`, `isready`, `position`, `go`, etc.).

The engine also supports:

- `d` — print the board
- `eval` — print an evaluation breakdown
- `go perft <n>` — run a perft to depth n
- `full perft` — run perft 1 through 7

## Training the network

Inside `neural-network/`:

```
python -m venv venv
source venv/bin/activate
pip install -r requirements.txt
python main.py     # trains and saves chess_net.pt
python export.py   # exports to chess_net.onnx
```

Training data is in `X.npy` (encoded positions) and `y.npy` (labels).

## Playing on lichess

The `lichess-bot/` directory contains a lichess-bot setup pointing at the engine binary. See lichess-bot's own documentation for configuring the token and engine path.

import torch
import torch.nn as nn


def decode_fen(fen: str) -> torch.Tensor:
    parts = fen.split(" ")
    board_str = parts[0]
    side_to_move = 1.0 if parts[1] == "w" else 0.0

    inputs = [0.0] * 768

    rank = 7
    file = 0
    for c in board_str:
        if c == "/":
            rank -= 1
            file = 0
        elif c.isdigit():
            file += int(c)
        else:
            color = 0 if c.isupper() else 1
            piece = "pbnrqk".index(c.lower())
            square = rank * 8 + file
            inputs[color * 384 + piece * 64 + square] = 1.0
            file += 1

    inputs.append(side_to_move)
    return torch.tensor(inputs, dtype=torch.float32)

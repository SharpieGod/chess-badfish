import torch
import torch.nn as nn
from torch.utils.data import Dataset, DataLoader


def decode_fen(fen: str) -> torch.Tensor:
    parts = fen.split(" ")
    side = parts[1]
    castling = parts[2]
    en_passant = parts[3]
    fifty_move = int(parts[4]) if len(parts) > 4 else 0
    board_str = parts[0]

    board = [0.0] * 768

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
            board[color * 384 + piece * 64 + square] = 1.0
            file += 1

    side_node = [1.0 if side == "w" else 0.0]

    castling_nodes = [
        1.0 if "K" in castling else 0.0,
        1.0 if "Q" in castling else 0.0,
        1.0 if "k" in castling else 0.0,
        1.0 if "q" in castling else 0.0,
    ]

    ep_nodes = [0.0] * 8
    if en_passant != "-":
        ep_nodes["abcdefgh".index(en_passant[0])] = 1.0
    fifty_node = [min(fifty_move, 100) / 100.0]

    inputs = board + side_node + castling_nodes + ep_nodes + fifty_node

    return torch.tensor(inputs, dtype=torch.float32)


class ChessNet(nn.Module):
    def __init__(self):
        super().__init__()
        self.net = nn.Sequential(
            nn.Linear(782, 256),  # 6 * 2 * 64 + 1 + 4 + 8 + 1
            nn.ReLU(),
            nn.Dropout(0.3),
            nn.Linear(256, 128),
            nn.ReLU(),
            nn.Dropout(0.2),
            nn.Linear(128, 64),
            nn.ReLU(),
            nn.Linear(64, 1),
            nn.Sigmoid(),
        )

    def forward(self, x):
        return self.net(x)


class ChessDataset(Dataset):
    def __init__(self, filepath):
        self.samples = []
        with open(filepath, "r") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                fen, score = line.rsplit(" | ", 1)
                self.samples.append((fen, float(score)))

    def __len__(self):
        return len(self.samples)

    def __getitem__(self, idx):
        fen, score = self.samples[idx]
        x = decode_fen(fen)
        y = torch.tensor([score], dtype=torch.float32)
        return x, y


if __name__ == "__main__":
    # Actual training
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(torch.cuda.is_available())
    print(torch.cuda.get_device_name(0))
    print(f"{device}")

    import numpy as np
    import os

    if not os.path.exists("X.npy"):
        print("Decoding FENs...")
        raw = ChessDataset("/home/darko/projects/chess-monkfish/training-data.txt")
        n = len(raw)

        X = np.memmap("X.npy", dtype="float32", mode="w+", shape=(n, 782))
        y = np.memmap("y.npy", dtype="float32", mode="w+", shape=(n,))

        chunk_size = 50_000
        for i in range(0, n, chunk_size):
            end = min(i + chunk_size, n)
            X[i:end] = torch.stack([raw[j][0] for j in range(i, end)]).numpy()
            y[i:end] = torch.tensor([raw[j][1] for j in range(i, end)]).numpy()
            print(f"  {end}/{n}")
        print("Saved")
    else:
        print("Loading...")
        X = np.memmap("X.npy", dtype="float32", mode="r", shape=(10_000_000, 782))
        y = np.memmap("y.npy", dtype="float32", mode="r", shape=(10_000_000,))

    dataset = torch.utils.data.TensorDataset(
        torch.from_numpy(np.array(X)), torch.from_numpy(np.array(y))
    )
    train_size = int(0.95 * len(dataset))
    val_size = len(dataset) - train_size
    train_set, val_set = torch.utils.data.random_split(dataset, [train_size, val_size])
    train_loader = DataLoader(train_set, batch_size=8192, shuffle=True)
    val_loader = DataLoader(val_set, batch_size=8192)

    model = ChessNet().to(device)
    if __name__ == "main":
        optimizer = torch.optim.Adam(model.parameters(), lr=1e-3)
        scheduler = torch.optim.lr_scheduler.ReduceLROnPlateau(
            optimizer, mode="min", factor=0.5, patience=3
        )
        criterion = nn.BCELoss()

        best_val = float("inf")
        patience = 5
        no_improve = 0
        print("starting training")

        for epoch in range(100):
            model.train()
            train_loss = 0.0
            for x, y in train_loader:
                x, y = x.to(device), y.to(device)
                optimizer.zero_grad()
                pred = model(x).squeeze(1)
                loss = criterion(pred, y)
                loss.backward()
                optimizer.step()
                train_loss += loss.item()
            model.eval()
            val_loss = 0.0

            with torch.no_grad():
                for x, y in val_loader:
                    x, y = x.to(device), y.to(device)
                    pred = model(x).squeeze(1)
                    val_loss += criterion(pred, y).item()

                val_loss_avg = val_loss / len(val_loader)
                print(
                    f"Epoch {epoch+1:02d} | "
                    f"Train: {train_loss/len(train_loader):.4f} | "
                    f"Val: {val_loss_avg:.4f}"
                )

                scheduler.step(val_loss_avg)

                if val_loss_avg < best_val:
                    best_val = val_loss_avg
                    torch.save(model.state_dict(), "chess_net.pt")
                    no_improve = 0
                else:
                    no_improve += 1
                    if no_improve >= patience:
                        print(
                            f"Early stopping at epoch {epoch+1}, best val: {best_val:.4f}"
                        )
                        break

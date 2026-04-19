import torch
import torch.nn as nn
from torch.utils.data import Dataset, DataLoader
from main import ChessNet

device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
model = ChessNet().to(device)
model.load_state_dict(torch.load("chess_net.pt"))

dummy_input = torch.zeros(1, 782).to(device)

torch.onnx.export(
    model,
    dummy_input,
    "chess_net.onnx",
    input_names=["input"],
    output_names=["output"],
    dynamic_axes={"input": {0: "batch"}, "output": {0: "batch"}},
)

print("Exported to chess_net.onnx")

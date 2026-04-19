import torch

# Create a sample tensor
x = torch.rand(5, 3)
print(x)

# Check for GPU (NVIDIA) or MPS (Apple)
print(f"CUDA available: {torch.cuda.is_available()}")

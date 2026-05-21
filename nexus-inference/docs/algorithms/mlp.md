# MLP — Multi-Layer Perceptron

**Feedforward neural network.** Layers of neurons connected by weight
matrices, with nonlinear activation functions between layers. Learns
arbitrary continuous functions from data.

| Property | Value |
|----------|-------|
| Prediction cost | ~0.5 ns per FMA (scalar), dominated by matmul |
| Memory | `Σ(layer[i] x layer[i+1])` weights + `Σ(layer[i+1])` biases |
| Types | `MlpF64`, `MlpF32` |
| Construction | `from_parts(layer_sizes, weights, biases, activation)` |
| Output | Single scalar or multi-output vector |

## What It Does

```
  Input         Hidden layer (relu)      Output (linear)
  features      ┌────────────────┐       ┌──────────┐

   x0 ──────────┤ w*x + b → relu ├──┐
                └────────────────┘  │
   x1 ──────────┤ w*x + b → relu ├──┼────┤ w*h + b ├──── score
                └────────────────┘  │    └──────────┘
   x2 ──────────┤ w*x + b → relu ├──┘
                └────────────────┘

  Forward pass for one layer:
    output[j] = activation( bias[j] + Σ(weights[j,k] * input[k]) )

  Output layer has NO activation — produces raw linear scores.
  Caller applies sigmoid/softmax if needed (same as GBDT).
```

Each layer performs a matrix-vector multiply followed by an activation
function. The network topology is defined at construction by
`layer_sizes` — e.g., `[8, 16, 1]` means 8 inputs, 16 hidden neurons
with activation, 1 linear output.

## Weight Layout

Weights are stored **row-major (output-major)** — each row contains
the weights for one output neuron. This matches PyTorch's
`nn.Linear.weight` layout directly.

```
  Layer connecting 3 inputs to 2 outputs:

  weights = [w00, w01, w02,    ← row 0: weights for output neuron 0
             w10, w11, w12]    ← row 1: weights for output neuron 1

  output[0] = bias[0] + w00*in[0] + w01*in[1] + w02*in[2]
  output[1] = bias[1] + w10*in[0] + w11*in[1] + w12*in[2]
```

All weight matrices are concatenated into a single flat array,
layer by layer. Same for biases.

## Activation Functions

A single activation function is applied to all hidden layers.
The output layer is always linear.

| Activation | Formula | Feature required | Use case |
|-----------|---------|-----------------|----------|
| `Relu` | `max(0, x)` | None | Default, most common |
| `LeakyRelu(alpha)` | `x if x >= 0, alpha*x otherwise` | None | Prevents dead neurons |
| `Tanh` | `tanh(x)` | `std` or `libm` | Bounded output [-1, 1] |
| `Sigmoid` | `1 / (1 + exp(-x))` | `std` or `libm` | Bounded output [0, 1] |

**Design note:** The current API uses a single activation for the
entire model. Per-layer activations (e.g., relu hidden + tanh final
hidden) would require a builder API and is a potential future extension.

## NaN Handling

Two prediction modes:

| Method | NaN behavior | Cost |
|--------|-------------|------|
| `predict` | Scans inputs, returns `Err(NanInput)` | O(n_inputs) scan + matmul |
| `predict_unchecked` | NaN propagates through computation | matmul only |

NaN propagates correctly through all activations:
- **Relu**: NaN passes through (three-branch comparison, matches PyTorch)
- **LeakyRelu**: `NaN * alpha = NaN`
- **Tanh/Sigmoid**: transcendentals propagate NaN

Unlike GBDT, MLP has no learned NaN behavior — there is no meaningful
"default direction" for missing features. The checked path rejects
NaN; the unchecked path propagates it so the caller can detect it
in the output.

## Scratch Buffers

The forward pass needs intermediate storage between layers (ping-pong
buffers). Two `Vec` allocations happen per `predict_into_unchecked`
call, sized to the maximum layer dimension.

For small networks (8-64 neurons), this is 64-512 bytes and the
allocator typically reuses memory. For latency-critical paths where
even this matters, a future `predict_with_scratch` API could accept
caller-provided buffers.

## When to Use It

**Use MLP when:**
- You have a trained neural network from PyTorch/TensorFlow
- Inputs are dense numeric vectors (not sparse tabular features)
- The relationship is nonlinear and can't be tabulated
- Prediction budget is 100ns-2us (small networks, 1-3 hidden layers)

**Don't use MLP when:**
- Features are sparse/tabular with many categorical variables (use [GBDT](gbdt.md))
- The function can be precomputed over a small grid (use [LUT](lut.md))
- You need sub-10ns predictions (use [LUT](lut.md))
- Network has >64 neurons per layer and you need <500ns (needs SIMD, not yet implemented)

## Code Example

```rust
use nexus_inference::{MlpF64, Activation};

// 4 inputs → 8 hidden (relu) → 1 output
let model = MlpF64::from_parts(
    &[4, 8, 1],
    &weights,  // 4*8 + 8*1 = 40 weights, row-major
    &biases,   // 8 + 1 = 9 biases
    Activation::Relu,
).unwrap();

// Checked prediction (rejects NaN inputs)
let score = model.predict(&[0.5, 1.2, -0.3, 0.8]).unwrap();

// Unchecked (faster, NaN propagates)
let score = model.predict_unchecked(&[0.5, 1.2, -0.3, 0.8]);

// Multi-output
let model = MlpF64::from_parts(&[4, 8, 3], &w, &b, Activation::Relu).unwrap();
let mut output = [0.0_f64; 3];
model.predict_into_unchecked(&[0.5, 1.2, -0.3, 0.8], &mut output);
```

## Complexity

| Operation | Time | Space |
|-----------|------|-------|
| Construction | O(total_weights) | O(total_weights + total_biases) |
| `predict_unchecked` | O(Σ layer[i] x layer[i+1]) | O(max_layer_size) scratch |
| `predict` | O(n_inputs) + O(matmul) | O(max_layer_size) scratch |

The cost is dominated by FMA count:

| Topology | FMAs | Approx. latency |
|----------|------|----------------|
| 8→16→1 | 144 | ~100 ns |
| 16→32→8→1 | 776 | ~370 ns |
| 64→64→1 | 4,160 | ~2 us |

Latency scales linearly with FMA count at scalar throughput.
SIMD vectorization (AVX2, 4-wide f64) would reduce the larger
configurations by ~4x.

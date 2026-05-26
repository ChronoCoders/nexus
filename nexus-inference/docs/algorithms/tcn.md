# TCN — Temporal Convolutional Network

**A stack of dilated causal 1D convolutions.** Each layer doubles its
dilation, so the receptive field grows *exponentially* with depth while
cost grows only *linearly*. A few layers see hundreds of timesteps with a
fixed, known lookback — no recurrent state to leak or explode.

| Property | Value |
|----------|-------|
| Prediction cost | ~ num_layers × a causal-conv step (≈50-115 ns/layer; see [Causal1dConv](causal1d.md)) |
| Memory | 4 B × (Σ filters×kernel×in_ch + biases + output proj) + ring buffers |
| Type | `TinyTcn` (stateful — sliding window per layer) |
| Construction | `from_parts()` (per-layer conv weights + output projection) |
| Output | Single scalar, or multi-output via `predict_into` |

## What It Does

```
  u_t ─► conv_0 (dilation 1) ─► act ─► conv_1 (dilation 2) ─► act [+res] ─► ... ─► output_proj ─► y_t
          sees t, t-1, t-2        \________ sees t, t-2, t-4 ________/
          (kernel taps adjacent)            (kernel taps every 2nd step)
```

Layer `L` is a causal 1D convolution with **dilation `2^L`**: its kernel
taps reach back `2^L` steps between samples. Stacking layers compounds the
reach. With `kernel_size` taps per layer and `num_layers` layers:

```
  receptive_field = 1 + (kernel_size − 1) × (2^num_layers − 1)
```

So kernel 3, 4 layers → reach of `1 + 2×15 = 31` steps from 4 cheap layers;
6 layers → 127 steps. **Exponential reach, linear cost.** Each layer keeps
a small circular buffer of past inputs; a step writes the newest sample and
convolves. Optional **residual** connections add the layer input to its
output where dimensions match (layers 1+, and layer 0 when
`input_size == filters`), which helps gradients during training and
preserves the raw signal.

## When to Use It

**Use TCN when:**
- You need **medium-to-long range** temporal patterns with a **fixed,
  known lookback** — the receptive field is exact and auditable, not a
  learned, fuzzy memory.
- A single [Causal1dConv](causal1d.md) doesn't reach far enough, but you
  don't want recurrent state.
- You want **convolutional determinism** — no gates, bounded per-step cost,
  reset semantics that are easy to reason about.
- You trained a **TCN in PyTorch** (dilated `nn.Conv1d` stack).

**Don't use TCN when:**
- The pattern fits in a **short window** — one [Causal1dConv](causal1d.md)
  is cheaper.
- You need **unbounded / adaptive** memory whose horizon you can't fix in
  advance — use [LSTM](lstm.md) / [GRU](gru.md), or [SSM](ssm.md) for very
  long linear memory.
- Inputs are tabular snapshots, not sequences — use [GBDT](gbdt.md) /
  [MLP](mlp.md).

## Output Interpretation

`predict()` advances one timestep and returns `y_t` (fp32). State persists
across calls. Two things to respect:

- **Priming.** Until `receptive_field()` steps have been fed, the deepest
  layer's window isn't full and the output is based on zero-padded history.
  Check `is_primed()` before trusting predictions; gate downstream logic on it.
- **Reset.** Call `reset()` at a sequence boundary (new instrument, data
  gap, session start) to clear all layer buffers.

## Code Example

```rust
use nexus_inference::{Activation, TinyTcn};

let filters = 4;
let kernel_size = 3;

// Per-layer conv weights, (filters, kernel_size, in_ch) row-major.
// Layer 0: in_ch = input_size (2); layers 1+: in_ch = filters.
let w0 = vec![0.1_f32; filters * kernel_size * 2];        // layer 0 (dilation 1)
let b0 = vec![0.0_f32; filters];
let w1 = vec![0.1_f32; filters * kernel_size * filters];  // layer 1 (dilation 2)
let b1 = vec![0.0_f32; filters];
let w_out = vec![0.1_f32; 1 * filters];                   // (output_size, filters)
let b_out = vec![0.0_f32; 1];

let mut tcn = TinyTcn::from_parts(
    2, filters, kernel_size, 1, /* residual */ false,
    &[&w0, &w1], &[&b0, &b1],
    &w_out, &b_out,
    Activation::Relu,
).unwrap();

if tcn.is_primed() {                  // false until receptive_field() steps in
    let _y = tcn.predict(&[0.5, 1.0]);
}
let _rf = tcn.receptive_field();      // 1 + (3-1)*(2^2 - 1) = 7
tcn.reset();                          // clear at a sequence boundary
```

## Complexity

| Operation | Time | Space |
|-----------|------|-------|
| Construction (`from_parts`) | O(total_weights) | O(total_weights) |
| `predict` (one step) | O(num_layers × filters × kernel × channels) | O(receptive window) |

Per-step cost is roughly `num_layers ×` a single causal-conv layer (≈50-115
ns each — see [Causal1dConv](causal1d.md)), so a 2-3 layer TCN lands in the
~100-350 ns range. The win is the scaling: doubling the receptive field
costs **one** more layer (linear), not double the work.

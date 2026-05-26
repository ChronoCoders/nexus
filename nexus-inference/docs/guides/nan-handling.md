# NaN Handling

## The contract: `predict()` trusts its inputs

NaN/Inf validation is the **caller's responsibility** — the standard ML-library
convention. PyTorch, NumPy, and scikit-learn don't re-validate values on every
forward pass either; a per-element finite check is a real cost we don't pay on
the hot path.

Precisely, `predict()` / `predict_into()`:

- **validates length** — panics if input length ≠ the model's input size (a cheap
  `assert_eq!`).
- **does NOT validate values** — no NaN/Inf check; whatever you pass flows straight
  into the math.

The *only* value-level NaN logic in the crate is GBDT's opt-in `predict_nan_aware`.
Every other path assumes finite inputs. Construction (`from_parts`) does reject
non-finite *weights* — but that's a one-time setup check, not a per-call one.

## What each model does with a NaN input

There are **three** behaviors, and the grouping matters because **you cannot
uniformly rely on "a NaN will show up in the output."** For three of these models,
it won't.

### 1. Handles it — GBDT

Default `predict()` routes NaN right (`NaN <= threshold` is false) — a finite,
deterministic result, but not the learned-best direction. `predict_nan_aware()`
routes NaN via the **learned** default direction (LightGBM trains it), at ~30% more
cycles per node. GBDT is the one model that can be *correct* on missing features.

### 2. Propagates it — and stateful models poison their state

The output becomes NaN, which the caller can detect:

- **Stateless — MLP**: NaN in → NaN out. One bad input, one bad output, no aftermath.
- **Stateful — LSTM, GRU, StackedLstm, StackedGru, SSM, Causal1dConv, TinyTcn**: the
  NaN reaches the output *and corrupts carried state*:
  - **RNNs / SSM**: the hidden (and cell) state stays NaN for **every future step
    until `reset()`**. One bad input poisons the model indefinitely.
  - **Causal1dConv / TCN**: the NaN sits in the sliding buffer and corrupts outputs
    until it **slides out of the receptive window** (or permanently if it keeps
    re-entering).

  So for stateful models, one bad input is not one bad output — it's a burst or a
  permanent run of NaN. `reset()` is the recovery; clear state at a sequence boundary
  or after a detected NaN.

### 3. Silently absorbs it — LUT, BNN, QuantizedMlp (the trap)

These **swallow the NaN and return a confident, finite, wrong number.** Your
"detect a NaN in the output" safety net does **not** fire here:

- **LUT** → NaN maps to bin 0 (Rust's saturating float→int cast). A valid table
  entry, meaningless.
- **BNN** → the fp32 input layer propagates NaN, then sign-binarization (`x >= 0.0`,
  which is *false* for NaN) collapses it to **−1**. The bit pattern is definite; the
  prediction is silently wrong.
- **QuantizedMlp** → the input is quantized to i8 *first* (`NaN.round() as i32`
  saturates to 0 on the scalar path; `_mm256_cvtps_epi32(NaN)` = `i32::MIN` on the
  SIMD path), then integer matmul cannot produce NaN. Definite, silently wrong — and
  the scalar and SIMD builds may even map NaN to *different* i8 values.

## Per-type summary

| Type | NaN input produces | NaN visible in output? |
|------|--------------------|------------------------|
| GBDT | `predict`: routes right; `predict_nan_aware`: learned routing | n/a (handled) |
| MLP | NaN output | **yes** |
| LSTM / GRU / StackedLstm / StackedGru | NaN output + state poisoned until `reset()` | yes, then stuck |
| SSM | NaN output + state poisoned until `reset()` | yes, then stuck |
| Causal1dConv / TCN | NaN output until it slides out of the window | yes, transient |
| LUT | bin 0 — finite, wrong | **NO** |
| BNN | −1 at binarization — finite, wrong | **NO** |
| QuantizedMlp | a definite i8 — finite, wrong | **NO** |

## Where to validate

NaN validation belongs in the **feature pipeline**, not at the inference boundary:

```
  Feature producer  →  Validate / impute  →  Inference
  (may produce NaN)    (catch NaN HERE)      (clean inputs assumed)
```

This is doubly important given the three silent absorbers: if a NaN reaches LUT,
BNN, or QuantizedMlp, nothing downstream will tell you. And for stateful models, a
NaN that slips through doesn't just produce one bad number — it can disable the
model until you `reset()` it.

## Code Patterns

### Standard hot path

```rust
// Feature pipeline guarantees clean inputs; predict() trusts them.
let score = model.predict(&features);
```

### GBDT with missing features

```rust
// GBDT routes NaN via its learned default direction.
let gbdt_score = gbdt_model.predict_nan_aware(&features);

// Everything else expects clean inputs — impute upstream.
let features_clean = impute_nan(&features);
let mlp_score = mlp_model.predict(&features_clean);
```

### Stateful recovery after a detected NaN

```rust
let y = lstm.predict(&features);
if y.is_nan() {
    lstm.reset();   // clear poisoned hidden/cell state before the next sequence
}
```

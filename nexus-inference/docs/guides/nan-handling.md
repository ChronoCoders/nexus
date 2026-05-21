# NaN Handling

All three model types follow a **checked/unchecked** pattern for NaN
inputs. The checked path is the safe default; the unchecked path is
the fast path for callers who validate inputs upstream.

## Per-Type NaN Contracts

| Type | `predict` (checked) | `predict_unchecked` |
|------|-------------------|-------------------|
| GBDT | Routes NaN via learned default direction | No NaN check, undefined routing |
| MLP | Returns `Err(NanInput)` | NaN propagates through computation |
| LUT | Returns `Err(NanInput)` | NaN maps to bin 0 (silent wrong answer) |

**GBDT is the exception.** It *handles* NaN rather than rejecting it.
LightGBM learns the optimal NaN direction during training, so
`predict()` produces meaningful results even with missing features.
The cost is ~30% more cycles per node (NaN comparison overhead).

MLP and LUT *reject* NaN in the checked path because there is no
learned or mathematically correct behavior for missing inputs.

## Where to Validate

In production systems, NaN validation belongs in the **feature
pipeline**, not at the inference boundary:

```
  Feature producer  →  Validate/impute  →  Inference
  (may produce NaN)    (catch NaN here)    (clean inputs)
```

The checked `predict` methods are a safety net, not the primary
defense. The unchecked methods assume the pipeline has done its job.

## MLP NaN Propagation (Unchecked Path)

When NaN enters an MLP via `predict_unchecked`, it propagates
through all operations:

- **Matmul**: `NaN * weight = NaN`, `NaN + bias = NaN`
- **Relu**: NaN passes through (IEEE 754: `NaN > 0.0` is false,
  `NaN <= 0.0` is false, third branch returns NaN)
- **LeakyRelu**: `NaN * alpha = NaN`
- **Tanh/Sigmoid**: transcendentals propagate NaN

The output will be NaN, which the caller can detect. This is
the correct behavior — NaN propagation preserves the "something
is wrong" signal. No silent data corruption.

## LUT NaN Behavior (Unchecked Path)

Rust's saturating float-to-int cast maps `NaN as usize` to 0.
So NaN features always index bin 0 for that dimension. The result
is a valid number from the table — but meaningless. This is silent
data corruption, which is why the checked path exists.

## Code Patterns

### Belt and suspenders (recommended for new code)

```rust
// Feature pipeline validates, inference checks as safety net
let score = model.predict(&features).map_err(|e| {
    tracing::error!("NaN reached inference: {e}");
    e
})?;
```

### Hot path with upstream validation

```rust
// Feature pipeline guarantees clean inputs
// Skip the scan — every nanosecond counts
let score = model.predict_unchecked(&features);
```

### Mixed: GBDT handles NaN, MLP rejects it

```rust
// GBDT is NaN-tolerant (routes via default direction)
let gbdt_score = gbdt_model.predict(&features);

// MLP requires clean inputs
let features_clean = impute_nan(&features);
let mlp_score = mlp_model.predict_unchecked(&features_clean);
```

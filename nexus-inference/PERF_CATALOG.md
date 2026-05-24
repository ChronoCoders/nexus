# nexus-inference Performance Optimization Catalog

Systematic record of SIMD and algorithmic optimizations applied to each
model type. Intended as an audit reference so future work doesn't
rediscover dead ends or miss context on why something was done.

All SIMD work targets AVX2+FMA with AVX-512F where available. Scalar
fallbacks exist for all code paths. Benchmarks run pinned (`taskset -c 0`)
with turbo boost disabled where noted.

---

## Shared: dot product primitives (`src/dot/`)

The foundation everything else builds on. All model types bottleneck on
matrix-vector products, so dot product throughput is the single largest
lever.

### dot_f32 / dot_f64 — single dot product

- **4 independent accumulators** to hide FMA latency (4-5 cycle on most
  x86). Inner loop processes 32 f32s (4×8-wide FMA) or 16 f64s (4×4-wide
  FMA) per iteration.
- Unrolled main loop + 8-element cleanup loop + scalar tail.
- AVX2 and AVX-512 implementations with compile-time dispatch via `cfg`.

### dot4_f32 / dot4_f64 — 4 simultaneous dot products

- **Shared input loads**: one `_mm256_loadu` per input vector feeds all 4
  row accumulators. Cuts input bandwidth by 4×.
- **2 accumulators per row (A/B split, 8 total)** to hide FMA latency.
  Inner loop processes 16 f32s or 8 f64s per iteration.
- Scalar tail for remainder.

### dot4_f32_m128 — 4 dots returning packed `__m128`

- Same accumulation as `dot4_f32` but returns `__m128` instead of
  `[f32; 4]`.
- **Paired hadd reduction**: `lo0/lo1 → hadd → h01`, `lo2/lo3 → hadd →
  h23`, then `hadd(h01, h23)` produces all 4 sums in the target lanes.
  11 reduction instructions vs 28 for 4 separate `hsum_f32` calls.
- Enables callers (matvec, MLP tiled, conv tiled) to fuse
  bias-add + activation + store in SIMD without scalar round-trip.
- LLVM never inlines this (~225 asm instructions). `#[inline]` hint is
  present but the function is too large. This is fine — the function call
  overhead is amortized over the inner loop work.

### dot8_f32_m256 — 8 simultaneous dot products (newest)

- **8 independent accumulators** (1 per row), single input load per
  iteration. 8 FMA chains hide latency without A/B splitting.
- Returns `__m256` for direct store or fused operations.
- **Reduction**: 8 cross-lane folds (`extractf128 + add`), 3 levels of
  `hadd`, final `insertf128` to pack `__m256`.
- **AVX-512 variant**: 16 `__m512` accumulators (A/B split per row),
  processes 32 elements per inner iteration. 3-stage reduction:
  `__m512 → __m256 → __m128 → hadd → insertf128`.
- **Threshold gating**: only called when `in_size >= 32`. Below that, the
  heavier reduction cost isn't amortized and `dot4_f32_m128` is faster.
  Verified empirically: `in_size=24` regresses ~13%, `in_size=40` improves
  ~9%, `in_size=80` improves ~18%.

### matvec_bias_f32 / matvec_f32 — tiled matrix-vector product

- Outer loop: dot8 (8 rows at a time) when `in_size >= 32`, then dot4
  (4 rows), then scalar tail.
- `#[inline]` — inlined into LSTM/GRU gate computation for zero call
  overhead.
- Used by: LSTM, GRU, stacked LSTM/GRU, Conv output projection.

---

## GBDT (`src/gbdt.rs`)

### False-branch-next node layout

- Compact 16-byte `Node` struct (`repr(C)`): feature_idx (u16), left (u16),
  flags (u16), value (f64). The `right` child field is **absent**.
- DFS right-first tree reordering: the false/right child is always at
  `idx + 1`. Eliminates a stored index per node and makes ~50% of
  decisions (the false path) sequential — served from L1 by the hardware
  prefetcher.
- `reorder_and_compact()` converts from `RawNode` (explicit left/right)
  to this layout during model construction.

### 12-byte packed layout (rejected)

- Benchmarked `repr(C, packed)` at 12 bytes per node (no padding after
  `flags`). The 25% smaller working set doesn't shift the L2-vs-L3 cache
  tier for any tested configuration, and the non-power-of-2 stride (×12 vs
  ×16) plus unaligned access overhead **regressed L2-resident cases by
  ~25%**. 16-byte aligned is the measured optimum.

### predict_n — partial ensemble

- `predict_n(features, n_trees)` sums only the first `n` trees. Enables
  early-stopping at inference time and A/B testing of ensemble depth.

---

## LUT (`src/lut.rs`)

O(1) prediction via discretized feature lookup. No SIMD optimization
needed — the operation is a division + array index per feature, already
<10ns. Perf-sensitive work here is table construction, not prediction.

---

## MLP (`src/mlp.rs`)

### SIMD tiled path (f32 only)

- `mlp_tiled_simd_f32`: `#[inline(never)]` free function processing the
  `out_size_4` portion of each layer.
- **Fused bias + activation + store** in SIMD registers. No scalar
  round-trip between dot product and activation.
- Relu path: `_mm_max_ps(bias + dots, zero)` (or `_mm256_max_ps` for dot8).
- Identity/last-layer path: `bias + dots` directly.
- **dot8→dot4 cascade** with `in_size >= 32` threshold: groups of 8 use
  `dot8_f32_m256`, remainder of 4 uses `dot4_f32_m128`.
- `mlp_tiled_noop<T>`: generic no-op for `MlpF64` (returns 0, compiler
  eliminates). Keeps the macro signature uniform without dead-code in the
  f64 path.

### 3-branch borrow checker pattern

- `predict_into` dispatches to `$tiled_fn` with one of three disjoint
  src/dst pairs: `(scratch_a → scratch_b)`, `(scratch_b → scratch_a)`, or
  `(scratch → output)`. Each branch is separate so Rust proves disjoint
  borrows. One branch per layer, not per element.

### cfg-gated `let mut j` pattern

- `#[cfg(SIMD)] let mut j = { tiled_fn(...) };` and
  `#[cfg(not(SIMD))] let mut j = 0usize;` avoids `unused_assignments`
  warnings from the scalar fallback overwriting a previously-assigned `j`.

### Measured results (f32 SIMD tiled, pre-dot8)

Pinned, vs scalar baseline:

| Config | Improvement |
|--------|-------------|
| 8→16→1 | 27% |
| 16→32→8→1 | 33% |
| 64→64→1 | 37% |
| 32→32→32→32→1 | 41% |
| 64→64→64→1 | 39% |

Stacked (deeper) configs benefit more — the tiled path runs per layer.

### f64 — no SIMD tiled path

`MlpF64` uses the generic `dot4_f64` + scalar activation fallback. The
tiled approach would work but f64 MLP is not a hot-path use case. If
needed, add `mlp_tiled_simd_f64` following the f32 pattern.

---

## LSTM (`src/rnn/lstm.rs`, `src/rnn/avx2_gates.rs`, `src/rnn/avx512_gates.rs`)

### Architecture

Two hot operations per step:
1. **Gate matvec**: `matvec_bias_f32(w_ih, concat(input, hidden))` →
   4H-dimensional gate vector. This is the bottleneck (~80%+ of step time
   for hidden ≥ 32).
2. **Gate activation + cell/hidden update**: sigmoid(i,f,o), tanh(g),
   cell update, tanh(cell) → hidden update.

### SIMD gate processing (AVX2)

- `lstm_gates_avx2`: processes 8 hidden units at a time.
- **Padé [7,6] rational approximation** for tanh — 7th degree numerator,
  6th degree denominator. Evaluated with FMA chains (3 FMA per
  num/den). Accuracy ~1e-5 max error over [-4.97, 4.97].
- **NaN preservation**: `_mm256_cmp_ps(x, x, _CMP_UNORD_Q)` detects NaN
  lanes before clamping, then `_mm256_blendv_ps` restores them. Without
  this, `min/max` clamping silently converts NaN to clip values.
- **Sigmoid via tanh**: `0.5 + 0.5 * tanh(x * 0.5)`. One function, not
  two approximations.
- Cell update: `c_new = fg * c_old + ig * cg` — single FMA.
- Hidden update: `h_new = og * tanh(c_new)` — reuses tanh_8wide.
- Scalar tail for `hidden % 8 != 0`.

### SIMD gate processing (AVX-512)

- `lstm_gates_avx512`: same algorithm, 16 lanes at a time.
- `tanh_16wide` / `sigmoid_16wide` using `__m512` intrinsics.
- NaN detection via `_mm512_cmp_ps_mask` + `_mm512_mask_blend_ps`
  (k-mask variant).

### matvec improvements (from dot8)

The gate matvec calls `matvec_bias_f32` which now uses the
dot8→dot4 cascade. For LSTM 16→64→1 (gate matrix 256×80):

| Config | Improvement |
|--------|-------------|
| 4→8→1 | -7% |
| 8→16→1 | -2% (neutral) |
| 8→32→1 | -10% |
| 16→64→1 | -13% |
| 8→32×2L | -12% |
| 8→32×3L | -13% |

### What wasn't done (and why)

- **Fused gate matvec**: computing the full gate matrix as a single fused
  operation (interleaving matvec rows with activation) would avoid writing
  the intermediate gate buffer. Not done because (a) the gate buffer is
  hot in L1 immediately after matvec, so the reload is ~4 cycles, and
  (b) fusing would prevent code reuse with the shared `matvec_bias_f32`.
- **Quantized weights (int8)**: would halve memory bandwidth but requires
  scale factors, dequantization overhead, and complicates the loader.
  Worth revisiting if models grow beyond L2.
- **Blocked/tiled matvec for cache**: the gate matrices are small enough
  (max ~256×80 = 80KB at f32) to fit in L2. Cache-blocking would add
  complexity without benefit at these sizes.

---

## GRU (`src/rnn/gru.rs`, `src/rnn/avx2_gates.rs`, `src/rnn/avx512_gates.rs`)

### Architecture

Three hot operations per step:
1. **input-hidden matvec**: `matvec_f32(w_ih, input)` → 3H gate vector
   (no bias — bias is applied during gate activation).
2. **hidden-hidden matvec**: `matvec_f32(w_hh, hidden)` → 3H gate vector.
3. **Gate activation + hidden update**: computes reset, update, candidate
   gates and blends old/new hidden state.

GRU splits the matvec into two calls (input-hidden and hidden-hidden)
because the candidate gate applies the reset gate between them:
`n = tanh(ih_cand + r * hh_cand)`. This is inherent to the GRU
architecture and can't be fused into a single matvec.

### SIMD gate processing

- `gru_gates_avx2` / `gru_gates_avx512`: 8-wide / 16-wide processing.
- Same Padé tanh/sigmoid as LSTM (shared functions).
- Reset gate: `r = sigmoid(ih + bias_ih + hh + bias_hh)` — 4 loads + 3
  adds + sigmoid.
- Update gate: same structure.
- Candidate: `n = tanh(ih_cand + bias_ih + r * (hh_cand + bias_hh))` —
  FMA for reset-gated term.
- Hidden blend: `h' = (1-z)*n + z*h` — sub + FMA.

### matvec improvements (from dot8)

GRU uses `matvec_f32` (no-bias variant):

| Config | Improvement |
|--------|-------------|
| 8→16→1 | +3% (noise) |
| 8→32→1 | -5% |
| 16→64→1 | -13% |
| 8→32×2L | -7% |
| 8→32×3L | -5% |

### What wasn't done (and why)

- **Fused matvec** (same reasoning as LSTM — buffer fits in L1).
- **Single matvec for all gates**: GRU's reset-gate-before-candidate
  structure prevents this. The candidate's hidden-hidden contribution
  depends on `r`, which depends on the first matvec.

---

## Causal 1D Convolution (`src/conv/causal1d.rs`)

### Architecture

Two phases per step:
1. **Convolution**: `n_filters` dot products over the linearized circular
   buffer (length = `kernel_size × input_channels`).
2. **Output projection**: `matvec_bias_f32(w_out, filter_scratch)`.

### Circular buffer linearization

- Maintains a circular write buffer of the last `kernel_size` inputs.
- Each step linearizes into `lin_buf` before convolution. The memcpy
  cost is small (typically 16-128 f32s) and enables contiguous dot
  products without modular indexing in the inner loop.

### SIMD tiled convolution

- `conv_tiled_simd`: `#[inline(never)]` free function.
- **dot8→dot4 cascade** with `conv_len >= 32` threshold.
- **Fused bias + activation + store**: Relu path uses
  `_mm256_max_ps(bias + dots, zero)` / `_mm_max_ps` variant. Identity
  path skips the max.
- Handles Relu and Identity activations in SIMD. Other activations fall
  through to scalar.

### Measured results (dot8 cascade)

| Config | Improvement |
|--------|-------------|
| 4ch×4k×8f (conv_len=16) | ~0% (below threshold) |
| 4ch×8k×16f (conv_len=32) | -5% |
| 8ch×8k×32f (conv_len=64) | -16% |

### What wasn't done (and why)

- **im2col / GEMM-based convolution**: standard for large CNNs but
  overkill for our use case (small kernel sizes, streaming single-step).
  The linearized dot product approach has no materialization overhead.
- **Winograd convolution**: only helps for kernel_size=3 or 5, and the
  overhead dominates at our filter counts.

---

## Stacked LSTM / Stacked GRU (`src/rnn/stacked_lstm.rs`, `src/rnn/stacked_gru.rs`)

Same optimizations as single-layer variants — they call the same
`matvec_bias_f32` / `matvec_f32` and gate functions. The stacked models
benefit more from dot8 because non-first layers have `in_size =
hidden + hidden` (typically ≥ 32), clearing the threshold.

---

## Cross-cutting: things that apply everywhere

### `#[inline(never)]` for SIMD helpers

Both `mlp_tiled_simd_f32` and `conv_tiled_simd` use `#[inline(never)]`.
LLVM otherwise inlines these large functions into every call site,
bloating the caller's instruction footprint. The function call overhead
(~5 cycles) is negligible relative to the matvec work.

### Compile-time SIMD dispatch

All SIMD paths are selected at compile time via `cfg(target_feature)`,
not runtime `is_x86_feature_detected!()`. This means:
- Zero runtime dispatch cost.
- Build with `RUSTFLAGS="-C target-cpu=native"` for best codegen.
- The binary is not portable across CPU generations (acceptable for
  inference workloads deployed to known hardware).

### Scalar fallbacks

Every SIMD function has a scalar fallback compiled on non-x86 or
non-AVX2 targets. The scalar paths use the same algorithmic structure
(dot4 tiling, multi-accumulator) so correctness tests cover both paths.

### Activation functions

- Relu: `max(x, 0)` — trivially vectorized as `_mm*_max_ps(x, zero)`.
- Identity: no-op — just bias-add.
- Tanh/Sigmoid: Padé [7,6] rational approximation (LSTM/GRU gates).
  Not yet vectorized in MLP/Conv paths — those use scalar `activate_f32`
  for non-Relu activations. Vectorizing Tanh/Sigmoid for MLP would help
  if those activations become common in deployed models.

---

## Benchmark methodology

```bash
# Build
RUSTFLAGS="-C target-cpu=native" cargo bench --bench temporal_bench -p nexus-inference --no-run
RUSTFLAGS="-C target-cpu=native" cargo bench --bench predict_bench -p nexus-inference --features loader-lightgbm --no-run

# Baseline
taskset -c 0 ./target/release/deps/temporal_bench-* --bench --save-baseline <name>

# Compare
taskset -c 0 ./target/release/deps/temporal_bench-* --bench --baseline <name>
```

Turbo boost should be disabled for stable results:
```bash
echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo
# ... run benchmarks ...
echo 0 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo
```

Without turbo disabled, individual runs can vary 5-15%. Always do A/B
comparison (`--save-baseline` → `--baseline`), never trust absolute
numbers from a single run.

---

## Summary: what moves the needle

| Optimization | Where | Impact |
|---|---|---|
| dot4 shared input loads | everywhere | foundational — 4× input bandwidth reduction |
| dot4_f32_m128 batched hadd | matvec, MLP, Conv | eliminates scalar hsum round-trip |
| dot8_f32_m256 (in_size≥32) | matvec, MLP, Conv | 5-18% on medium/large models |
| Padé tanh/sigmoid 8-wide | LSTM/GRU gates | eliminates scalar activation bottleneck |
| MLP fused bias+relu in SIMD | MLP f32 | 27-41% vs scalar |
| Conv fused bias+relu in SIMD | Conv f32 | 5-16% vs scalar |
| GBDT false-branch-next layout | GBDT | ~50% of traversals sequential in L1 |
| `#[inline(never)]` on tiled helpers | MLP, Conv | prevents caller I-cache bloat |

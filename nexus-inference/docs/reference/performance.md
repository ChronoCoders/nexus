# Performance

All benchmarks uncontrolled (no turbo disable, no core pinning).
Relative comparisons valid within each column. For controlled
measurements, see the [benchmarking guide](../../CLAUDE.md).

## GBDT

| Configuration | `predict_unchecked` | `predict` (NaN-aware) | NaN overhead |
|--------------|-------------------:|---------------------:|------------:|
| 50 trees x depth 6, 8 features | 218 ns | — | — |
| 100 trees x depth 6, 8 features | 409 ns | 1.01 us | ~2.5x |
| 200 trees x depth 8, 16 features | 2.21 us | — | — |

Per-node cost: **~4.7 cycles** (unchecked), **~6 cycles** (NaN-aware).

At L1 load latency of 4 cycles, the unchecked path is within ~1 cycle
of the hardware floor for data-dependent tree traversal. The
false-branch-next layout ensures ~50% of traversal steps are
sequential (served by hardware prefetcher from L1).

### GBDT optimization history

| Optimization | Impact |
|-------------|--------|
| Bounds check elimination (`get_unchecked`) | ~20% reduction |
| Flat storage (single `Box<[Node]>` + offset table) | ~10% reduction |
| NaN `partial_cmp` restructure | ~10% reduction on NaN path |
| False-branch-next DFS layout | ~25% reduction (largest single win) |
| **Total** | **~54% reduction from baseline** |

12-byte packed nodes (25% smaller working set) and 4-wide interleaved
tree walks were tried and rejected — both regressed performance.
See [perf.md](../../.claude/perf.md) for detailed analysis.

## MLP

| Configuration | FMAs | `predict_unchecked` |
|--------------|-----:|-------------------:|
| 8→16→1 relu | 144 | 99 ns |
| 16→32→8→1 relu | 776 | 372 ns |
| 64→64→1 relu | 4,160 | 2.00 us |

Cost scales linearly with FMA count at scalar throughput.

### FMA throughput analysis

At 3.2 GHz with scalar f64 FMA (~2 cycles throughput for a
reduction chain):

| Configuration | Expected (arithmetic only) | Measured | Overhead |
|--------------|-------------------------:|--------:|---------:|
| 8→16→1 | 90 ns | 99 ns | ~10% (Vec alloc) |
| 16→32→8→1 | 485 ns | 372 ns | Faster (LLVM optimization) |
| 64→64→1 | 2,600 ns | 2,000 ns | Faster (better than expected) |

The Vec allocation overhead is visible for small networks but
negligible for larger ones. SIMD vectorization (AVX2, 4-wide f64)
would reduce all configurations by ~4x.

### NaN scan overhead

The checked `predict` adds `input.iter().any(|x| x.is_nan())`
before the forward pass. For 8-64 features, this is 8-64 `ucomisd`
self-comparisons — negligible relative to hundreds of FMAs.

## LUT

| Configuration | `predict_unchecked` |
|--------------|-------------------:|
| 2 features x 10 bins | 4.9 ns |
| 3 features x 20 bins | 7.5 ns |

LUT prediction is dominated by the per-feature division
`(value - min) / step`. LLVM may convert this to multiply-by-reciprocal
when the step is constant.

## Complexity Summary

| Type | Predict | Construction |
|------|---------|-------------|
| GBDT | O(trees x depth) | O(total_nodes) |
| MLP | O(Σ layer[i] x layer[i+1]) | O(total_weights) |
| LUT | O(n_features) | O(n_bins^n_features) |

## Memory

| Type | Formula | Example |
|------|---------|---------|
| GBDT | 16B/node + 4B/tree | 100 trees x 63 nodes = 101 KB |
| MLP | 8B/weight + 8B/bias + 2B/layer | 8→16→1: 328 B |
| LUT | 8B/entry + 8B/feature x 2 + 3B | 2 feat x 10 bins: 835 B |

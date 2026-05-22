# Choosing a Model Type

## Decision Tree

```
  What are you deploying?
  │
  ├── A trained LightGBM model
  │   └── GBDT (use from_lightgbm)
  │
  ├── A trained neural network (PyTorch, TF, etc.)
  │   └── MLP (export weights to from_parts)
  │
  ├── A pre-computed function over a small grid
  │   └── LUT (tabulate in Python, load flat array)
  │
  └── Not sure yet — what matters most?
      │
      ├── Latency under 10ns
      │   └── LUT (O(1), ~5ns for 2 features)
      │
      ├── Tabular features with missing values
      │   └── GBDT (learned NaN routing)
      │
      ├── Dense numeric inputs, nonlinear relationships
      │   └── MLP (universal function approximation)
      │
      └── Simple monotonic relationship, 1-2 features
          └── LUT (precompute, avoid model complexity)
```

## Comparison

| Criterion | GBDT | MLP | LUT |
|-----------|------|-----|-----|
| **Input type** | Tabular features | Dense vectors | 1-3 numeric features |
| **Latency** | 200ns - 3us | 100ns - 2us | 5-10ns |
| **Missing data** | Learned NaN routing | No (reject or propagate) | No (clamp to bin 0) |
| **Output** | Single scalar | Single or multi-output | Single scalar |
| **Model source** | LightGBM | PyTorch/TF/sklearn | Python script |
| **Accuracy** | As trained | As trained | Bin resolution |
| **Memory** | 16B/node | 8B/weight | 8B/bin^features |
| **Loader** | `from_lightgbm()` | `from_parts()` | `from_parts()` |

## When to Combine Types

In trading systems, it's common to use multiple model types together:

- **GBDT for feature selection** → extract top features, feed into MLP
  for final prediction
- **LUT for fast pre-filters** → coarse signal check in <10ns, then
  GBDT/MLP for the full model only when the filter fires
- **MLP for embeddings** → neural network produces a dense vector,
  GBDT consumes it as features alongside tabular data

The `predict_into` API makes composition straightforward — one model's
output buffer feeds directly into the next model's input.

## Model Size Guidelines

| Type | Small | Medium | Large |
|------|-------|--------|-------|
| GBDT | 50 trees x depth 6 (~220ns) | 100 x 6 (~410ns) | 200 x 8 (~2.2us) |
| MLP | 8→16→1 (~100ns) | 16→32→8→1 (~370ns) | 64→64→1 (~2us) |
| LUT | 1 feat x 10 bins | 2 feat x 10 bins | 3 feat x 20 bins |

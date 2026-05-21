# no_std Support

nexus-inference supports `no_std` environments through feature flags.

## Feature Flag Hierarchy

```
  std (default)
   └── alloc
        ├── libm (optional)
        └── loader-lightgbm (optional)
```

| Flag | What it enables |
|------|----------------|
| `std` | Standard library, implies `alloc` |
| `alloc` | All model types (GBDT, MLP, LUT) — requires `Box`, `Vec` |
| `libm` | `Tanh`/`Sigmoid` activations without `std` (uses `libm` crate) |
| `loader-lightgbm` | `GbdtF64::from_lightgbm()` text parser |

## Minimum: `alloc` only

```toml
[dependencies]
nexus-inference = { version = "0.1", default-features = false, features = ["alloc"] }
```

This gives you all three model types with `Relu` and `LeakyRelu`
activations. `Tanh` and `Sigmoid` are rejected at construction time
(`from_parts` returns `LoadError::Validation`).

## With transcendental activations

```toml
[dependencies]
nexus-inference = { version = "0.1", default-features = false, features = ["libm"] }
```

The `libm` feature implies `alloc` and adds the `libm` crate for
`tanh()` and `exp()` implementations. Same API, same results — just
a different math backend.

## Without `alloc`

```toml
[dependencies]
nexus-inference = { version = "0.1", default-features = false }
```

This compiles but provides no model types — only the error types
(`LoadError`, `NanInput`). Useful if you need the error types in a
shared crate that's `no_std` without an allocator.

## What uses `alloc`

| Component | Allocation |
|-----------|-----------|
| Model structs | `Box<[T]>` for weights, biases, nodes, tables |
| `from_parts()` | Copies input slices into owned storage |
| `from_lightgbm()` | Parses text, builds node vectors |
| MLP `predict_into_unchecked()` | Two `Vec` scratch buffers per call |
| `LoadError`, `NanInput` | No allocation (stack types) |

The only per-prediction allocation is MLP's scratch buffers. GBDT
and LUT allocate nothing after construction.

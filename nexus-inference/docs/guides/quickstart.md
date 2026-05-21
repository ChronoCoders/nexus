# Quickstart

## Add the dependency

```toml
[dependencies]
nexus-inference = { version = "0.1", features = ["loader-lightgbm"] }
```

## Load and predict with a GBDT

```rust
use nexus_inference::GbdtF64;

// Load from LightGBM text format (model_text.txt)
let bytes = std::fs::read("model_text.txt").unwrap();
let model = GbdtF64::from_lightgbm(&bytes).unwrap();

// Predict — NaN-aware (routes via learned default direction)
let features = vec![0.5, 1.2, -0.3, 0.8, 2.1, 0.0, -1.5, 3.3];
let score = model.predict(&features);

// Predict — unchecked (faster, caller guarantees no NaN)
let score = model.predict_unchecked(&features);
```

## Load and predict with an MLP

```rust
use nexus_inference::{MlpF64, Activation};

// Weights exported from PyTorch (see python-export.md)
let layer_sizes = &[4, 8, 1];  // 4 inputs → 8 hidden → 1 output
let weights: Vec<f64> = load_weights();  // 4*8 + 8*1 = 40 values
let biases: Vec<f64> = load_biases();    // 8 + 1 = 9 values

let model = MlpF64::from_parts(
    layer_sizes, &weights, &biases, Activation::Relu,
).unwrap();

// Checked — returns Err(NanInput) if any input is NaN
let score = model.predict(&[0.5, 1.2, -0.3, 0.8]).unwrap();

// Unchecked — NaN propagates through computation
let score = model.predict_unchecked(&[0.5, 1.2, -0.3, 0.8]);
```

## Load and predict with a LUT

```rust
use nexus_inference::LutF64;

// Pre-computed table: 2 features, 10 bins each
let table: Vec<f64> = load_table();  // 100 values

let model = LutF64::from_parts(
    2,              // n_features
    10,             // n_bins
    &[0.0, 0.0],   // feature minimums
    &[1.0, 1.0],   // feature maximums
    &table,
).unwrap();

// Checked — returns Err(NanInput) if any feature is NaN
let value = model.predict(&[0.35, 0.72]).unwrap();

// Unchecked — NaN maps to bin 0 (silent wrong answer)
let value = model.predict_unchecked(&[0.35, 0.72]);
```

## Multi-output MLP

```rust
use nexus_inference::{MlpF64, Activation};

// 4 inputs → 8 hidden → 3 outputs
let model = MlpF64::from_parts(
    &[4, 8, 3], &weights, &biases, Activation::Relu,
).unwrap();

// predict() panics for multi-output — use predict_into
let mut output = [0.0_f64; 3];
model.predict_into(&[0.5, 1.2, -0.3, 0.8], &mut output).unwrap();
// output[0], output[1], output[2] now contain the three predictions
```

## Handling errors

```rust
use nexus_inference::{MlpF64, Activation, NanInput, LoadError};

// Construction errors
let result = MlpF64::from_parts(&[2, 0, 1], &[], &[], Activation::Relu);
match result {
    Err(LoadError::Validation(msg)) => eprintln!("bad model: {msg}"),
    Err(LoadError::Parse(msg)) => eprintln!("parse error: {msg}"),
    Ok(model) => { /* use model */ }
}

// Prediction errors (checked path only)
let model = MlpF64::from_parts(&[2, 1], &[1.0, 1.0], &[0.0], Activation::Relu).unwrap();
match model.predict(&[f64::NAN, 1.0]) {
    Err(NanInput) => eprintln!("input contains NaN"),
    Ok(score) => println!("score: {score}"),
}
```

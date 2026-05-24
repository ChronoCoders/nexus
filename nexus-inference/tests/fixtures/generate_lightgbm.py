#!/usr/bin/env python3
"""Generate LightGBM test fixtures for nexus-inference integration tests.

Trains small LightGBM models with deterministic seeds, saves the model in
text format, runs predictions, and records expected outputs as JSON.

This closes the loop: train in LightGBM Python -> load in nexus-inference
Rust via from_lightgbm -> verify predictions match.

Generated files are checked in (no Python/LightGBM dependency for cargo test).
Re-run this script manually when the loader changes.

Install dependencies:
    pip install lightgbm numpy
"""

import json
import math

import lightgbm as lgb
import numpy as np
from pathlib import Path

FIXTURES_DIR = Path(__file__).parent


def make_regression_data(n_samples, n_features, seed):
    rng = np.random.RandomState(seed)
    X = rng.uniform(0, 10, (n_samples, n_features))
    coeffs = np.array([(-1) ** i * (i + 1) for i in range(n_features)], dtype=np.float64)
    y = X @ coeffs + rng.normal(0, 0.5, n_samples)
    return X, y


def make_binary_data(n_samples, n_features, seed):
    rng = np.random.RandomState(seed)
    X = rng.uniform(-5, 5, (n_samples, n_features))
    logits = X[:, 0] - 0.5 * X[:, 1] + 0.3 * rng.normal(0, 1, n_samples)
    y = (logits > 0).astype(int)
    return X, y


def make_test_inputs(n_inputs, n_features, seed, lo=-5.0, hi=10.0):
    mid = (lo + hi) / 2
    scale = (hi - lo) / 2
    inputs = []
    for i in range(n_inputs):
        row = []
        for j in range(n_features):
            val = mid + scale * math.sin(seed * (i + 1) * (j + 1) * 0.7)
            row.append(round(val, 8))
        inputs.append(row)
    return inputs


def generate_model(name, X_train, y_train, params, test_inputs,
                   nan_indices=None, tolerance=1e-10):
    """Train a LightGBM model and save fixtures.

    Args:
        name: fixture name prefix
        X_train: training features
        y_train: training labels
        params: LightGBM parameters
        test_inputs: list of input vectors for prediction
        nan_indices: list of (input_idx, feature_idx) to set to NaN
        tolerance: comparison tolerance for Rust tests
    """
    params.setdefault("num_threads", 1)
    dataset = lgb.Dataset(X_train, label=y_train, free_raw_data=False)
    model = lgb.train(params, dataset, num_boost_round=params.get("n_trees", 10))

    model_path = FIXTURES_DIR / f"lgb_{name}.txt"
    model.save_model(str(model_path))

    inputs_array = np.array(test_inputs, dtype=np.float64)

    if nan_indices:
        for input_idx, feat_idx in nan_indices:
            inputs_array[input_idx, feat_idx] = np.nan

    raw_preds = model.predict(inputs_array, raw_score=True)

    serializable_inputs = []
    for row in inputs_array:
        serializable_inputs.append([None if np.isnan(v) else float(v) for v in row])

    outputs = raw_preds.tolist()
    if not isinstance(outputs, list):
        outputs = [outputs]

    with open(FIXTURES_DIR / f"lgb_{name}_expected.json", "w") as f:
        json.dump(
            {
                "inputs": serializable_inputs,
                "outputs": outputs,
                "tolerance": tolerance,
                "n_features": int(X_train.shape[1]),
                "n_trees": int(model.num_trees()),
                "objective": params.get("objective", "regression"),
            },
            f,
            indent=2,
        )
        f.write("\n")

    n_feat = X_train.shape[1]
    n_trees = model.num_trees()
    print(f"  lgb_{name}: {n_trees} trees, {n_feat} features, "
          f"{len(test_inputs)} test inputs")


def generate_regression_small():
    X, y = make_regression_data(100, 4, seed=42)
    params = {
        "objective": "regression",
        "num_leaves": 8,
        "n_trees": 5,
        "learning_rate": 0.3,
        "verbose": -1,
        "deterministic": True,
        "seed": 42,
    }
    inputs = make_test_inputs(10, 4, seed=1)
    generate_model("regression_small", X, y, params, inputs)


def generate_regression_deep():
    X, y = make_regression_data(200, 8, seed=43)
    params = {
        "objective": "regression",
        "num_leaves": 32,
        "max_depth": 5,
        "n_trees": 20,
        "learning_rate": 0.1,
        "verbose": -1,
        "deterministic": True,
        "seed": 43,
    }
    inputs = make_test_inputs(15, 8, seed=2)
    generate_model("regression_deep", X, y, params, inputs)


def generate_regression_large():
    X, y = make_regression_data(500, 16, seed=44)
    params = {
        "objective": "regression",
        "num_leaves": 64,
        "max_depth": 8,
        "n_trees": 50,
        "learning_rate": 0.05,
        "verbose": -1,
        "deterministic": True,
        "seed": 44,
    }
    inputs = make_test_inputs(20, 16, seed=3)
    generate_model("regression_large", X, y, params, inputs)


def generate_binary_small():
    X, y = make_binary_data(200, 4, seed=50)
    params = {
        "objective": "binary",
        "num_leaves": 8,
        "n_trees": 10,
        "learning_rate": 0.2,
        "verbose": -1,
        "deterministic": True,
        "seed": 50,
    }
    inputs = make_test_inputs(10, 4, seed=4)
    generate_model("binary_small", X, y, params, inputs)


def generate_binary_deep():
    X, y = make_binary_data(300, 6, seed=51)
    params = {
        "objective": "binary",
        "num_leaves": 16,
        "max_depth": 4,
        "n_trees": 15,
        "learning_rate": 0.15,
        "verbose": -1,
        "deterministic": True,
        "seed": 51,
    }
    inputs = make_test_inputs(12, 6, seed=5)
    generate_model("binary_deep", X, y, params, inputs)


def generate_nan_regression():
    """Regression with NaN inputs to test default_left routing."""
    X, y = make_regression_data(200, 4, seed=60)
    X_with_nan = X.copy()
    rng = np.random.RandomState(60)
    mask = rng.random(X.shape) < 0.15
    X_with_nan[mask] = np.nan

    params = {
        "objective": "regression",
        "num_leaves": 8,
        "n_trees": 10,
        "learning_rate": 0.2,
        "verbose": -1,
        "deterministic": True,
        "num_threads": 1,
        "seed": 60,
    }
    dataset = lgb.Dataset(X_with_nan, label=y, free_raw_data=False)
    model = lgb.train(params, dataset, num_boost_round=10)

    model_path = FIXTURES_DIR / "lgb_nan_regression.txt"
    model.save_model(str(model_path))

    inputs = make_test_inputs(10, 4, seed=6)
    nan_indices = [(1, 0), (1, 2), (3, 1), (5, 0), (5, 1), (5, 3), (7, 2)]

    inputs_array = np.array(inputs, dtype=np.float64)
    for input_idx, feat_idx in nan_indices:
        inputs_array[input_idx, feat_idx] = np.nan

    raw_preds = model.predict(inputs_array, raw_score=True)

    serializable_inputs = []
    for row in inputs_array:
        serializable_inputs.append([None if np.isnan(v) else float(v) for v in row])

    with open(FIXTURES_DIR / "lgb_nan_regression_expected.json", "w") as f:
        json.dump(
            {
                "inputs": serializable_inputs,
                "outputs": raw_preds.tolist(),
                "tolerance": 1e-10,
                "n_features": 4,
                "n_trees": model.num_trees(),
                "objective": "regression",
                "has_nan": True,
                "nan_indices": nan_indices,
            },
            f,
            indent=2,
        )
        f.write("\n")

    print(f"  lgb_nan_regression: {model.num_trees()} trees, 4 features, "
          f"{len(inputs)} test inputs ({len(nan_indices)} NaN entries)")


def generate_stump():
    """Minimal model: 1 tree, depth 1 (2 leaves)."""
    X, y = make_regression_data(50, 3, seed=70)
    params = {
        "objective": "regression",
        "num_leaves": 2,
        "n_trees": 1,
        "learning_rate": 1.0,
        "min_data_in_leaf": 1,
        "verbose": -1,
        "deterministic": True,
        "seed": 70,
    }
    inputs = make_test_inputs(8, 3, seed=7)
    generate_model("stump", X, y, params, inputs)


def generate_many_features():
    """Few trees but many features."""
    n_features = 32
    X, y = make_regression_data(300, n_features, seed=80)
    params = {
        "objective": "regression",
        "num_leaves": 16,
        "n_trees": 5,
        "learning_rate": 0.3,
        "verbose": -1,
        "deterministic": True,
        "seed": 80,
    }
    inputs = make_test_inputs(10, n_features, seed=8)
    generate_model("many_features", X, y, params, inputs)


def generate_huber():
    """Huber loss (robust regression) — different objective, same output format."""
    X, y = make_regression_data(200, 6, seed=90)
    params = {
        "objective": "huber",
        "huber_delta": 1.0,
        "num_leaves": 12,
        "n_trees": 10,
        "learning_rate": 0.2,
        "verbose": -1,
        "deterministic": True,
        "seed": 90,
    }
    inputs = make_test_inputs(10, 6, seed=9)
    generate_model("huber", X, y, params, inputs)


if __name__ == "__main__":
    print("Generating LightGBM fixtures...")
    generate_regression_small()
    generate_regression_deep()
    generate_regression_large()
    generate_binary_small()
    generate_binary_deep()
    generate_nan_regression()
    generate_stump()
    generate_many_features()
    generate_huber()
    print("Done.")

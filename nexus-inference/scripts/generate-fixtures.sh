#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
VENV_DIR="$CRATE_DIR/.venv"
FIXTURES_DIR="$CRATE_DIR/tests/fixtures"

if [ ! -d "$VENV_DIR" ]; then
    echo "Creating venv at $VENV_DIR ..."
    python3 -m venv --system-site-packages "$VENV_DIR"
fi

echo "Installing dependencies ..."
"$VENV_DIR/bin/pip" install -q \
    torch --index-url https://download.pytorch.org/whl/cpu
"$VENV_DIR/bin/pip" install -q \
    safetensors packaging numpy lightgbm==4.6.0

echo ""
echo "=== Generating safetensors fixtures (PyTorch) ==="
"$VENV_DIR/bin/python" "$FIXTURES_DIR/generate_all.py"

echo ""
echo "=== Generating LightGBM fixtures ==="
"$VENV_DIR/bin/python" "$FIXTURES_DIR/generate_lightgbm.py"

echo ""
echo "Done. Run 'cargo test -p nexus-inference --all-features' to verify."

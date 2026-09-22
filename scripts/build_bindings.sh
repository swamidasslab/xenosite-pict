#!/usr/bin/env bash
# Build shared depiction bindings for Python and/or JS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

usage() {
  echo "Usage: $0 [python|wasm|all]" >&2
  exit 1
}

target="${1:-all}"

build_python() {
  echo "==> maturin develop (xpict._native)"
  if command -v uv >/dev/null 2>&1; then
    uv run maturin develop --manifest-path crates/xpict-py/Cargo.toml
  else
    maturin develop --manifest-path crates/xpict-py/Cargo.toml
  fi
}

build_wasm() {
  echo "==> wasm-pack → js/src/wasm"
  mkdir -p js/src/wasm
  # Preserve stubs if pack fails mid-run
  wasm-pack build crates/xpict-wasm \
    --target web \
    --out-dir "$ROOT/js/src/wasm" \
    --out-name xpict_core
  rm -f js/src/wasm/.gitignore js/src/wasm/package.json js/src/wasm/README.md
}

case "$target" in
  python) build_python ;;
  wasm) build_wasm ;;
  all)
    build_python
    build_wasm
    ;;
  *) usage ;;
esac

echo "Done ($target)."

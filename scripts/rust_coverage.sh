#!/usr/bin/env bash
# Line coverage for xpict-core. CI fails under 93% (see .github/workflows/ci.yml).
# Vendored xenosmarts-syntax (`smiles_syntax/`, mostly generated `gen_smiles`)
# is excluded so the floor measures xpict paint/layout, not the snapshot.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
MIN="${RUST_COV_MIN:-93}"
IGNORE="${RUST_COV_IGNORE:-smiles_syntax/}"

# Prefer rustup toolchain + llvm-tools over Homebrew rust (often missing llvm-profdata).
# Keep ~/.cargo/bin on PATH so `cargo llvm-cov` finds the cargo-llvm-cov binary.
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

if command -v rustup >/dev/null 2>&1; then
  CARGO="$(rustup which cargo)"
  RUSTC="$(rustup which rustc)"
  export CARGO RUSTC
else
  CARGO="${CARGO:-cargo}"
  RUSTC="${RUSTC:-rustc}"
fi

HOST="$("$RUSTC" -vV | sed -n 's/^host: //p')"
LLVM_BIN="$("$RUSTC" --print sysroot)/lib/rustlib/${HOST}/bin"
if [[ -x "$LLVM_BIN/llvm-cov" && -x "$LLVM_BIN/llvm-profdata" ]]; then
  # Always bind to this rustc's llvm-tools (ignore stale Homebrew env vars).
  export LLVM_COV="$LLVM_BIN/llvm-cov"
  export LLVM_PROFDATA="$LLVM_BIN/llvm-profdata"
  export PATH="$LLVM_BIN:$PATH"
fi

if [[ -n "${LCOV_OUT:-}" ]]; then
  "$CARGO" llvm-cov -p xpict-core --lcov --output-path "$LCOV_OUT" \
    --ignore-filename-regex "$IGNORE" \
    --fail-under-lines "$MIN"
else
  "$CARGO" llvm-cov -p xpict-core --summary-only \
    --ignore-filename-regex "$IGNORE" \
    --fail-under-lines "$MIN"
fi

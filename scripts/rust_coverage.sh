#!/usr/bin/env bash
# Line coverage for xpict-core. CI fails under 97% (see .github/workflows/ci.yml).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
MIN="${RUST_COV_MIN:-97}"

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

# Include CoordGen modules (off by default for wasm-friendly builds).
if [[ -n "${LCOV_OUT:-}" ]]; then
  "$CARGO" llvm-cov -p xpict-core --features coordgen --lcov --output-path "$LCOV_OUT" \
    --fail-under-lines "$MIN"
else
  "$CARGO" llvm-cov -p xpict-core --features coordgen --summary-only --fail-under-lines "$MIN"
fi

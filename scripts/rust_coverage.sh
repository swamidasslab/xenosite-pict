#!/usr/bin/env bash
# Line coverage for xpict-core. CI fails under 97% (see .github/workflows/ci.yml).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
MIN="${RUST_COV_MIN:-97}"
cargo llvm-cov -p xpict-core --summary-only --fail-under-lines "$MIN"

#!/usr/bin/env bash
# Copy emscripten artifacts next to tsc output so published imports resolve.
set -euo pipefail
JS_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$JS_ROOT/src/wasm"
DST="$JS_ROOT/dist/wasm"
mkdir -p "$DST"
cp -a "$SRC"/xpict_core.js "$SRC"/xpict_core.d.ts "$SRC"/xpict_core.wasm "$DST"/
echo "Copied wasm → dist/wasm"

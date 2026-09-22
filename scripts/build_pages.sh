#!/usr/bin/env bash
# Assemble the GitHub Pages site under demo/_site (or $1).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-"$ROOT/demo/_site"}"

echo "==> build xpict package"
cd "$ROOT/js"
npm run build

echo "==> assemble Pages site → $OUT"
rm -rf "$OUT"
mkdir -p "$OUT/pkg"
cp "$ROOT/demo/index.html" "$ROOT/demo/demo.css" "$ROOT/demo/demo.js" "$OUT/"

# Copy ESM + wasm only (skip smoke / maps / d.ts)
while IFS= read -r -d '' f; do
  rel="${f#"$ROOT/js/dist/"}"
  case "$rel" in
    *.smoke.js) continue ;;
    native-fallback.js) continue ;;
  esac
  dest="$OUT/pkg/$rel"
  mkdir -p "$(dirname "$dest")"
  cp "$f" "$dest"
done < <(find "$ROOT/js/dist" \( -name '*.js' -o -name '*.wasm' \) -print0)

test -f "$OUT/pkg/wasm/xpict_core_bg.wasm"
test -f "$OUT/pkg/index.js"

echo "Pages site ready: $OUT"
du -sh "$OUT" "$OUT/pkg/wasm/xpict_core_bg.wasm"

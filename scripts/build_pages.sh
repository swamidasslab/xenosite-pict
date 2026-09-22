#!/usr/bin/env bash
# Assemble the GitHub Pages docs site (cross-language) under demo/_site or $1.
# Interactive JS demo is published at /js/demo/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-"$ROOT/demo/_site"}"

echo "==> build xpict JS package"
cd "$ROOT/js"
npm run build

echo "==> assemble Pages site → $OUT"
rm -rf "$OUT"
mkdir -p "$OUT"

# Cross-language docs from site/
cp -R "$ROOT/site/." "$OUT/"

# JS interactive demo → /js/demo/ (pkg next to demo.js)
DEMO_OUT="$OUT/js/demo"
mkdir -p "$DEMO_OUT/pkg"
cp "$ROOT/demo/index.html" "$ROOT/demo/demo.css" "$ROOT/demo/demo.js" "$DEMO_OUT/"

while IFS= read -r -d '' f; do
  rel="${f#"$ROOT/js/dist/"}"
  case "$rel" in
    *.smoke.js) continue ;;
    native-fallback.js) continue ;;
  esac
  dest="$DEMO_OUT/pkg/$rel"
  mkdir -p "$(dirname "$dest")"
  cp "$f" "$dest"
done < <(find "$ROOT/js/dist" \( -name '*.js' -o -name '*.wasm' \) -print0)

test -f "$DEMO_OUT/pkg/wasm/xpict_core_bg.wasm"
test -f "$DEMO_OUT/pkg/index.js"
test -f "$OUT/index.html"
test -f "$OUT/js/index.html"
test -f "$OUT/api.html"

# Compatibility: old root demo URL → JS demo
mkdir -p "$OUT/demo"
cat > "$OUT/demo/index.html" <<'EOF'
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="refresh" content="0; url=../js/demo/" />
    <link rel="canonical" href="../js/demo/" />
    <title>Redirecting…</title>
  </head>
  <body>
    <p>Demo moved to <a href="../js/demo/">/js/demo/</a>.</p>
  </body>
</html>
EOF

echo "Pages site ready: $OUT"
du -sh "$OUT" "$DEMO_OUT/pkg/wasm/xpict_core_bg.wasm"
find "$OUT" -maxdepth 2 -type f -name '*.html' | sort

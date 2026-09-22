#!/usr/bin/env bash
# Build the GitHub Pages site with MkDocs Material + cross-language autodoc.
#
# Steps:
#   1. Example SVG gallery (Python + RDKit when available)
#   2. JS package build (demo + TypeDoc inputs)
#   3. mkdocs build → $OUT
#   4. TypeDoc → $OUT/typedoc/
#   5. rustdoc → $OUT/rustdoc/
#   6. JS interactive demo → $OUT/js/demo/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-"$ROOT/_site"}"

cd "$ROOT"

echo "==> gallery SVGs"
if command -v uv >/dev/null 2>&1; then
  uv run --group docs --extra rdkit python scripts/generate_doc_examples.py \
    || uv run --group docs python scripts/generate_doc_examples.py
else
  python3 scripts/generate_doc_examples.py
fi

echo "==> JS package (wasm + tsc)"
cd "$ROOT/js"
if [[ -f package-lock.json ]]; then
  npm ci
else
  npm install
fi
export PATH="$HOME/.cargo/bin:$PATH"
if command -v wasm-pack >/dev/null 2>&1; then
  npm run build
else
  echo "wasm-pack missing; TypeScript-only build (demo wasm may be stale)"
  npm run build:ts
  npm run build:copy-wasm || true
fi
cd "$ROOT"

echo "==> MkDocs Material → $OUT"
rm -rf "$OUT"
uv run --group docs mkdocs build --site-dir "$OUT"

echo "==> TypeDoc → $OUT/typedoc"
cd "$ROOT/js"
npx --no-install typedoc \
  --entryPoints src/index.ts \
  --out "$OUT/typedoc" \
  --tsconfig tsconfig.json \
  --excludePrivate \
  --excludeExternals \
  --readme none \
  --name "@xenosite/xpict"
cd "$ROOT"

echo "==> rustdoc → $OUT/rustdoc"
cargo doc -p xpict-core --no-deps
cargo doc -p xpict --no-deps 2>/dev/null || true
rm -rf "$OUT/rustdoc"
mkdir -p "$OUT/rustdoc"
cp -a "$ROOT/target/doc/." "$OUT/rustdoc/"
touch "$OUT/rustdoc/.nojekyll"

echo "==> JS demo → $OUT/js/demo"
DEMO_OUT="$OUT/js/demo"
mkdir -p "$DEMO_OUT/pkg"
cp "$ROOT/demo/index.html" "$ROOT/demo/demo.css" "$ROOT/demo/demo.js" "$DEMO_OUT/"
if [[ -d "$ROOT/js/dist" ]]; then
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
fi

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

test -f "$OUT/index.html"
test -f "$OUT/gallery/index.html" || test -f "$OUT/gallery.html"
test -d "$OUT/typedoc"
test -d "$OUT/rustdoc/xpict_core"
echo "Pages site ready: $OUT"
du -sh "$OUT" || true

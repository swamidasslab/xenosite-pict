#!/usr/bin/env bash
# Regenerate live TypeScript + JSON Schema from xpict-core (schemars + ts-rs).
# Python live contracts stay hand-written for now — see docs/dev/typebridge.md.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> ts-rs export (codegen feature)"
cargo test -p xpict-core --features codegen --lib -- export_bindings 2>&1 | tail -40

BINDINGS="$ROOT/crates/xpict-core/bindings"
OUT_TS="$ROOT/js/src/generated"
mkdir -p "$OUT_TS"
# ts-rs writes one file per type under bindings/
if [[ -d "$BINDINGS" ]]; then
  # Flatten nested export dirs if any
  find "$BINDINGS" -name '*.ts' -print0 | while IFS= read -r -d '' f; do
    base="$(basename "$f")"
    cp "$f" "$OUT_TS/$base"
  done
  # Barrel for edge ABI
  {
    echo '// Auto-generated barrel — do not edit (make types).'
    echo '// Source: xpict-core edge + MoleculeIn (ts-rs).'
    echo
    for f in AlignOpts MolTemplate EdgeTask EdgePlan CoordMethod \
             CoordGenMoleculeResult EdgeTaskResult EdgeResult \
             AtomIn BondIn MoleculeIn; do
      if [[ -f "$OUT_TS/$f.ts" ]]; then
        echo "export type { $f } from \"./$f.js\";"
      fi
    done
  } > "$OUT_TS/edge.ts"
  echo "wrote $OUT_TS/*.ts"
else
  echo "error: no bindings/ from ts-rs" >&2
  exit 1
fi

echo "==> schemars live schemas"
cargo run -p xpict-core --example export_live_schema --features codegen --quiet

echo "Done. Commit js/src/generated/ and schema/edge-*.schema.json when intentional."

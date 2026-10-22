# xpict-wasm

Browser / Node bindings for [`xpict-core`](../xpict-core) via **wasm-bindgen**.

**Surface:** `render(spec)` (DepictSpec JSON → `Rendered[]`). `depictMolecule`
is a thin helper: wrap a lone mol as a mol spec and call `render`. The public
npm package is `xpict.mol` / `render` / `depict` / `toSvg`.

Depends on `xpict-core` with **`font`** (Liberation Sans) so heteroatom
labels (`OH`, `NH2`, …) place and paint. ELK stays off here (use npm `elkjs`).

## Build

From the repo root:

```bash
./scripts/build_bindings.sh wasm
# or
wasm-pack build crates/xpict-wasm --target web --out-dir ../../js/pkg --out-name xpict_core
```

JS imports from `@xenosite/xpict` → `dist/` + `dist/wasm/` (npmjs.org).

## Adding exports

1. Implement in `xpict-core`.
2. Prefer wrapping in **`xpict-py`** for the full surface.
3. Add a wasm bind here only if the JS MVP paint path needs it; keep
   `js/src/native.ts` internal (not a package export).

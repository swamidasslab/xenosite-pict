# xpict-wasm

Browser / Node bindings for [`xpict-core`](../xpict-core) via **wasm-bindgen**.

**MVP surface:** `depictMolecule` only (`MoleculeIn` JSON → `Scene` JSON).
The public npm package wraps that as `xpict.mol` / `render` / `toSvg`.
Broader helpers stay in `xpict-core` / PyO3.

Depends on `xpict-core` with **`font`** (Liberation Sans) so heteroatom
labels (`OH`, `NH2`, …) place and paint. ELK stays off here (use npm `elkjs`).

## Build

From the repo root:

```bash
./scripts/build_bindings.sh wasm
# or
wasm-pack build crates/xpict-wasm --target web --out-dir ../../js/pkg --out-name xpict_core
```

JS imports from `@swamidasslab/xpict` → `dist/` + `dist/wasm/` (GitHub Packages).

## Adding exports

1. Implement in `xpict-core`.
2. Prefer wrapping in **`xpict-py`** for the full surface.
3. Add a wasm bind here only if the JS MVP paint path needs it; keep
   `js/src/native.ts` internal (not a package export).

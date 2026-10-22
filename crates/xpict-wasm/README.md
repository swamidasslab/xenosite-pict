# xpict-wasm

Browser / Node bindings for [`xpict-core`](../xpict-core) via **Emscripten**
(`wasm32-unknown-emscripten`). CoordGen is C++; `em++` is the C++ compiler and
linker so Rust and CoordGen share one wasm linear memory.

**Surface:** `xpict_render` / `xpict_scene_to_svg` (C ABI; DepictSpec JSON →
`Rendered[]` JSON). JS `native.ts` wraps that as `renderJson`. The public npm
package is `xpict.mol` / `render` / `depict` / `toSvg`.

Depends on `xpict-core` with **`font`** (Liberation Sans) so heteroatom
labels (`OH`, `NH2`, …) place and paint.

## Build

Needs [emsdk](https://github.com/emscripten-core/emsdk) (`em++` on `PATH`).
`scripts/ensure_emsdk.sh` installs it under `~/.cache/emsdk`.

From the repo root:

```bash
./scripts/build_bindings.sh wasm
# or
make build-wasm
```

JS imports from `@xenosite/xpict` → `dist/` + `dist/wasm/` (npmjs.org).

## Adding exports

1. Implement in `xpict-core`.
2. Prefer wrapping in **`xpict-py`** for the full surface.
3. Add a C ABI export here only if the JS paint path needs it; keep
   `js/src/native.ts` internal (not a package export).

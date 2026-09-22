# xpict-wasm

Browser / Node bindings for [`xpict-core`](../xpict-core) via **wasm-bindgen**.
API mirrors Python `xpict._native` (same core functions).

## Build

From the repo root:

```bash
./scripts/build_bindings.sh wasm
# or
wasm-pack build crates/xpict-wasm --target web --out-dir ../../js/pkg --out-name xpict_core
```

JS imports from `@xenosite/xpict` → `src/native.ts`, which loads `js/pkg`.

## Adding exports

1. Implement in `xpict-core`.
2. Wrap in `xpict-py` **and** here with the same semantics.
3. Expose in `js/src/native.ts` and `src/xpict/native_bridge.py`.

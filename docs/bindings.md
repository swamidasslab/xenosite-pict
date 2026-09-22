# Dual language bindings

Shared depiction core in Rust, callable from **Python** and **JavaScript** with
the same function set.

```
                    ┌─────────────────┐
                    │  xpict-core     │
                    │  (pure Rust)    │
                    └────────┬────────┘
              ┌──────────────┼──────────────┐
              ▼                             ▼
     crates/xpict-py              crates/xpict-wasm
     (PyO3 / maturin)             (wasm-bindgen)
              ▼                             ▼
     xpict._native                js/src/wasm/xpict_core.*
              ▼                             ▼
     native_bridge.py             js/src/native.ts
```

## One-shot setup

```bash
# Python venv + Rust extension
uv sync --group dev
./scripts/build_bindings.sh all

# Verify
pytest tests/test_native_rust.py -q
cd js && npm test
```

| | Python | JavaScript |
| --- | --- | --- |
| Package | `xpict` | `@xenosite/xpict` |
| Init | (import extension) | `await initNative()` |
| Bond offset | `_native.multi_bond_offset(20)` | `multiBondOffset(20)` |
| PlotDot | `plotdot_rings(z)` | `plotdotRings(z)` → `{radiusFrac,colorZ}[]` |
| Capsule halo | `capsule_halo_path_d(...)` | `capsuleHaloPathD(...)` |
| Metrics | `_native.OFFSET_PX` | `offsetPx()` |

JS `plotdotDisks` takes `coords: [x,y][]`; the wasm layer flattens to
`[x,y,x,y,…]` for the Rust `Vec<f64>` ABI.

## Adding a shared export

1. `xpict-core` + Rust unit test  
2. Wrap in **both** `xpict-py` and `xpict-wasm`  
3. Expose via `native_bridge.py` and `js/src/native.ts`  
4. Parity test in Python; `js/src/native.smoke.mjs` for a quick wasm check  

See also [`crates/xpict-core/README.md`](../crates/xpict-core/README.md).

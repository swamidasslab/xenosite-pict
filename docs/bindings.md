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

## Roadmap: kill Shapely / fontTools on the ship path

Unifying Py + JS means **fonts and geometry live in Rust**, not parallel JS ports
of fontTools/Shapely.

| Step | Work | Exit criteria |
| --- | --- | --- |
| 1 | Capsule / disk halo SVG paths | Done (`geom` + bindings) |
| 2 | Polygon buffer + difference (glyph counters) | `halo_from_shapes` / `O` hole tests via `_native` |
| 3 | Liberation Sans outlines → path `d` + advances | `label_outline` / caption glyphs via Rust |
| 4 | Python draw calls only `native_bridge` for ink/text | Shapely **gone**; `fonttools` until `font` lands |
| 5 | JS `native.ts` exposes the same glyph/halo APIs | Browser labels match Python gallery |

Crates to prefer when filling stubs: **`ttf-parser`/`skrifa`** (fonts),
**`geo` + `i_overlay`** (boolean + buffer). Keep Liberation files under
`src/xpict/data/fonts/` (or `crates/xpict-core/fonts/`) so both bindings share bytes.

## Coords + alignment (RDKit at the edges, Rust for shared math)

**Focus:** each language calls **RDKit** for 2D layout and template alignment.
**Never** put RDKit inside `xpict-core` (won't WASM cleanly).

| Runtime | 2D coords | Template align | Then |
| --- | --- | --- | --- |
| Python | `backend="rdkit"` (`xpict[rdkit]`) | `align_rdkit.RdkitAligner` | Draw / rigid helpers via `_native` |
| Browser | `@rdkit/rdkit` MinimalLib | same package | `initNative()` draw / rigid helpers |

**Indigo alternate:** `backend="indigo"` + **fake/rigid align** in Rust only
(no RDKit template). Rigid-only also when RDKit is absent.

See `docs/layout-notes.md`.

## Diagram layout (ELK → elkrs)

Network / reaction viewport placement moves into Rust via **`elkrs`**
(`xpict-core` feature `elk`, default on for the lib / PyO3):

| Edge | API |
| --- | --- |
| Python | `_native.elk_layout_json(graph_json) -> str` |
| JS / WASM | npm `elkjs`; `xpict-wasm` uses `default-features = false` (no ELK/geom in the depict blob) |

Python synthesizes the ELK graph in `diagram/elk.py` and calls native only
(jsrun removed).

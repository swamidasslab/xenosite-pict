# Dual language bindings

Shared depiction core in Rust, callable from **Python** and **JavaScript** with
the same function set.

```
                    ┌─────────────────┐
                    │  xpict-core     │
                    │  (pure Rust)    │
                    └────────┬────────┘
              ┌──────────────┼──────────────┬──────────────────┐
              ▼              ▼              ▼                  ▼
     crates/xpict-py  crates/xpict-wasm  crates/xpict    (JS npm package)
     (PyO3 / maturin)  (wasm-bindgen)   (RDKit layout     (@swamidasslab/xpict)
                                           + public API)     RDKit.js + wasm
```

`crates/xpict` is the **native Rust** public package (mirrors JS/Python
`mol` / `render` / `to_svg`). It depends on crates.io `rdkit` for parse + a
local Depictor FFI for 2D/align. It is **not** linked into `xpict-py` or
`xpict-wasm`, so RDKit stays out of those artifacts.

## One-shot setup

```bash
# Python venv + Rust extension
uv sync --group dev
./scripts/build_bindings.sh all

# Verify
pytest tests/test_native_rust.py -q
cd js && npm test
cargo test -p xpict-core
# optional native package (system RDKit):
cargo test -p xpict
```

| | Python | JavaScript (MVP) | Native Rust |
| --- | --- | --- | --- |
| Package | `xpict` | `@swamidasslab/xpict` | crates.io `xpict` |
| Public API | `Mol` / `render` / `to_svg` (client) | `xpict.mol` / `render` / `toSvg` / `depict` | same + `depict` batch stub |
| Paint ABI | `_native.depict_molecule` | wasm `depictMolecule` (internal) | `xpict_core::depict_molecule` |
| Init | (import extension) | auto on first `render` | link-time RDKit |

**Publish / registries:** [`publish.md`](publish.md).

Python keeps the full `_native` surface (offsets, plotdots, halos, ELK, …).
JS MVP wasm only binds `depictMolecule`; RDKit layout/align stay in TS.
The native `crates/xpict` package is **not** linked into py/wasm.

## Adding a shared export

1. `xpict-core` + Rust unit test  
2. Wrap in **`xpict-py`** (and `xpict-wasm` only if the JS MVP paint path needs it)  
3. Expose via `native_bridge.py`; JS callers use `xpict.render` / scene JSON  
4. Parity test in Python; `js/src/api.smoke.ts` for the MVP path  

## Roadmap: kill Shapely / fontTools on the ship path

Unifying Py + JS means **fonts and geometry live in Rust**, not parallel JS ports
of fontTools/Shapely.

| Step | Work | Exit criteria |
| --- | --- | --- |
| 1 | Capsule / disk halo SVG paths | Done (`geom` + bindings) |
| 2 | Polygon buffer + difference (glyph counters) | `halo_from_shapes` / `O` hole tests via `_native` |
| 3 | Liberation Sans outlines → path `d` + advances | **Done** (`ttf-parser` + `_native`) |
| 4 | Python draw calls only `native_bridge` for ink/text | Shapely + fontTools **gone**; deps ≈ pydantic |
| 5 | JS paint uses Rust glyph/halo via `depictMolecule` | Browser labels match Python gallery |

Crates to prefer when filling stubs: **`ttf-parser`/`skrifa`** (fonts),
**`geo` + `i_overlay`** (boolean + buffer). Keep Liberation files under
`python/xpict/data/fonts/` (or `crates/xpict-core/fonts/`) so both bindings share bytes.

## Coords + alignment (RDKit at the edges, Rust for shared math)

**Focus:** each language calls **RDKit** for 2D layout and template alignment.
**Never** put RDKit inside `xpict-core` (won't WASM cleanly).

| Runtime | 2D coords | Template align | Then |
| --- | --- | --- | --- |
| Python | `backend="rdkit"` (`xpict[rdkit]`) | `align_rdkit.RdkitAligner` | Draw / rigid helpers via `_native` |
| Browser | `xpict.mol` / `render` / `toSvg` | RDKit align (hidden) | `scene` JSON → string |
| Native Rust | crates.io `rdkit` + `crates/xpict` Depictor FFI | `generateDepictionMatching2DStructure` | `xpict-core` paint → SVG |

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

## Scene document (shared depict → thin serializers)

**MVP:** single-molecule depiction with **marks** (circle atoms/bonds),
**shading** (plot-dot disks), backbone/label **color**, and **alignment**
(RDKit at edges; Rust rigid helpers). Not multi-mol / ELK scenes.

```
  RDKit / Indigo / native  (+ align)
           │  atoms, bonds, SVG-space coords, shade/marks
           ▼
     xpict-core  (paint → Scene, one viewport)
           ▼
  Python svg.py  ·  JS svg.ts   (thin: Scene → SVG / data-URI <img>)
```

`MoleculeIn` → (future) `depict_molecule` → `Scene` JSON is the ABI.
Port order: bond strokes/joins → marks → shade → full single-mol paint.

**ASAP track (xenosite.org client):** [`docs/migration-xenosite.md`](migration-xenosite.md).

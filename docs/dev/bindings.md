# Dual language bindings

Shared depiction core in Rust, callable from **Python** and **JavaScript** with
the same function set.

```
                    ┌─────────────────┐
                    │  xpict-core     │
                    │  (layout+paint) │
                    └────────┬────────┘
              ┌──────────────┼──────────────┬──────────────────┐
              ▼              ▼              ▼                  ▼
     crates/xpict-py  crates/xpict-wasm  crates/xpict    (JS npm package)
     (PyO3 / maturin)  (emscripten)    (public API)      (@xenosite/xpict)
```

`crates/xpict` is the **native Rust** public package (mirrors JS/Python
`mol` / `render` / `to_svg` / `depict`). Layout is CoordGen in `xpict-core`.
It is **not** linked into `xpict-py` or `xpict-wasm`.

## One-shot setup

```bash
make agent-install     # uv sync --group dev + maturin develop
make test              # or: make test-rust / test-python / test-js
```

`make help` lists build / check / pages / publish-dry targets. CI and Pages
workflows call the same recipes after installing toolchains.

| | Python | JavaScript (MVP) | Native Rust |
| --- | --- | --- | --- |
| Package | `xpict` | `@xenosite/xpict` | crates.io `xpict` |
| Public API | `Mol` / `render` / `to_svg` (client) | `xpict.mol` / `render` / `toSvg` / `depict` | same + `depict` batch stub |
| Paint ABI | `_native` render | wasm `xpict_render` (internal C ABI) | `xpict_core::doc::render` |
| Scene → SVG | `_native.scene_to_svg` | wasm `xpict_scene_to_svg` | `xpict_core::scene_to_svg` |
| Document | DepictSpec → `_native` / `depict` | wasm `render` / `xpict.depict` | `xpict::depict` / `doc::render` |
| Init | (import extension) | auto on first `render` | link-time |

**Publish / registries:** [`.github/PUBLISH.md`](../../.github/PUBLISH.md).

Python keeps the full `_native` surface (offsets, plotdots, halos, ELK, …).
JS wasm calls the same `render(spec)` path. Chrome (CX / star_labels / shade /
color) and EdgePlan validation are applied in core. The native `crates/xpict`
package is **not** linked into py/wasm.

### Where to test

| Logic | Test once in |
| --- | --- |
| CX parse / apply, `plan_edge`, `render_doc` chrome, EdgePlan validate | `xpict-core` |
| CoordGen `process_edge_plan` (atom_map, MCS, fallback) | `xpict-core` |
| Public `depict` / `render` wiring | one smoke per language |

Do **not** re-assert core behavior in Py + JS + Rust host suites.

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
| 4 | Hosts call Rust `scene_to_svg` (viewport `data-id` on `g.xpict-mol`) | No per-language SVG markup |
| 5 | JS paint uses Rust glyph/halo via wasm `render` | Browser labels match Python gallery |

Crates to prefer when filling stubs: **`ttf-parser`/`skrifa`** (fonts),
**`geo` + `i_overlay`** (boolean + buffer). Liberation files live under
`crates/xpict-core/fonts/` (packaged with crates.io).

## Coords + alignment (CoordGen)

**Focus:** every host uses **CoordGen** in `xpict-core` for 2D layout and
cost-based constrained align.

| Runtime | 2D coords | Template align | Then |
| --- | --- | --- | --- |
| Python | Rust `depict` (in-core CoordGen) | cost MCS + soft-constrained CoordGen | `_native` paint |
| Browser | wasm `renderJson` | same core align | `scene` JSON → string |
| Native Rust | `xpict::render` → core `depict` | same core align | `xpict-core` paint → SVG |

See [`layout-notes.md`](layout-notes.md).

## Diagram layout (ELK → elkrs)

Network / reaction viewport placement lives in Rust via **`elkrs`**
(always linked, including wasm). Hosts pass `DepictSpec`; they do not run ELK.

## Scene document (shared depict → Rust SVG)

Paint builds a `Scene`; **`xpict-core::scene_to_svg`** is the only markup
path. Viewport spec ids become `g.xpict-mol[data-id]`. Hosts wrap that
string (IPython, data-URI `<img>`).

**ASAP track (xenosite.org client):** [`migration-xenosite.md`](migration-xenosite.md).

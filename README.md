# xpict

Declarative, publication-quality small-molecule depiction for Python and the web.

Import path: `xpict`

## Status

Scaffold in progress. Language-neutral JSON contracts (Pydantic → generated JSON Schema) with Python and `js/` engines.

**Depiction:** Hard — follow CDK / RDKit / Indigo / CoordGen; don’t invent. Own SVG (skeleton → offsets → stereo). The product goal is a **native** depictor that is *demonstrably* good enough on a hard-case gallery.

**Ship stack:** **our** drawing (+ growing native layout) in Python now, migrating into **`xpict-core` Rust** (PyO3 + WASM) for dual Python/JS. No Chematic.

**Shared Rust core:** `crates/xpict-core` with **Python** (`xpict._native` / PyO3) and **JS** (wasm-bindgen → `js/src/native.ts`) bindings. Same callable surface on both sides — see [`docs/bindings.md`](docs/bindings.md).

**Next unify:** move **font** (Liberation outlines) and **geometry** (Shapely buffers / unions / counters) into `xpict-core`. That drops Python-only deps from the ship path and gives JS the same label/halo behavior without a second stack. Capsule/disk halos already call Rust; glyphs are next.

```bash
./scripts/build_bindings.sh all   # maturin + wasm-pack
pytest tests/test_native_rust.py
cd js && npm test
```

**Layout backends:** **native** (default). **Indigo** is an optional **alternate** (`xpict[indigo]`, `backend="indigo"`). **Alignment** (`diagram.align`): each language calls **RDKit its own way** (Python `rdkit`, JS `@rdkit/rdkit`) and passes maps/coords into Rust for shared transform math — **no RDKit inside `xpict-core` / WASM**. Rigid Kabsch remains the no-RDKit fallback.

**Multi-molecule diagrams:** ELK via **jsrun** (embedded V8 + vendored elkjs) — no Node required. Grid/row fallback if ELK fails.

**Core deps (target):** `pydantic` + `jsrun` + Rust extension. Shapely / fontTools become build-time-only once `geom`/`font` finish moving. Indigo / RDKit stay **language-edge extras**, not Rust crate deps.

**Outputs:** SVG (default); HTML with embedded SVG for responsive pages.

## Quick start

```python
from xpict import Pict, render

svg = Pict().render({"molecules": [{"smiles": "CCO"}]})  # native default
# or alternate layout:
svg = Pict(backend="indigo").render({"molecules": [{"smiles": "CCO"}]})
```

Input formats per molecule: `smiles`, `cxsmiles`, `esmiles`, or `molfile`.

Backend is runtime config (not in the JSON document). Unsupported options emit `PictBackendWarning`.

## Install

```bash
uv sync
uv sync --extra indigo   # alternate layout backend
uv sync --extra rdkit    # template alignment
uv sync --extra all
```

Export JSON Schema (committed under `schema/`):

```bash
uv run xpict-export-schema
```

## License

MIT

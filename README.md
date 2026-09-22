# xpict

Declarative, publication-quality small-molecule depiction for Python and the web.

Import path: `xpict`

## Status

Scaffold in progress. Language-neutral JSON contracts (Pydantic → generated JSON Schema) with Python and `js/` engines.

**Depiction:** Hard — follow CDK / RDKit / Indigo / CoordGen; don’t invent. Own SVG (skeleton → offsets → stereo). The product goal is a **native** depictor that is *demonstrably* good enough on a hard-case gallery.

**Chem stack (likely):** Chematic (or similar) for **perception** (aromaticity, SSSR, stereo — small Rust); **our** drawing. Chematic depict coords are not good enough. If we settle there, shipping depiction as a **Rust** crate (Py + WASM) is the natural fit — prove the draw/layout model in Python first.

**Shared Rust core:** `crates/xpict-core` with **Python** (`xpict._native` / PyO3) and **JS** (wasm-bindgen → `js/src/native.ts`) bindings. Same callable surface on both sides — see [`docs/bindings.md`](docs/bindings.md).

**Next unify:** move **font** (Liberation outlines) and **geometry** (Shapely buffers / unions / counters) into `xpict-core`. That drops Python-only deps from the ship path and gives JS the same label/halo behavior without a second stack. Capsule/disk halos already call Rust; glyphs are next.

```bash
./scripts/build_bindings.sh all   # maturin + wasm-pack
pytest tests/test_native_rust.py
cd js && npm test
```

**Layout coords (transitional):** **Indigo** for 2D coords while native matures. **Alignment** (`diagram.align`): prefer **RDKit** — template align exists in Python (`rdkit`) and in the browser (`@rdkit/rdkit` MinimalLib WASM). Rigid Kabsch remains the no-RDKit fallback. Chematic-as-layout stays out of the product path.

**Multi-molecule diagrams:** ELK via **jsrun** (embedded V8 + vendored elkjs) — no Node required. Grid/row fallback if ELK fails.

**Core deps (target):** `pydantic` + `jsrun` + Rust extension. Shapely / fontTools become build-time-only once `geom`/`font` finish moving. Indigo / RDKit / Chematic stay **extras**.

**Outputs:** SVG (default); HTML with embedded SVG for responsive pages.

## Quick start

```python
from xpict import Pict, render

svg = Pict(backend="indigo").render({"molecules": [{"smiles": "CCO"}]})
# or
svg = render({"molecules": [{"smiles": "CCO"}]}, backend="native")
```

Input formats per molecule: `smiles`, `cxsmiles`, `esmiles`, or `molfile`.

Backend is runtime config (not in the JSON document). Unsupported options emit `PictBackendWarning`.

## Install

```bash
uv sync
# transitional layout engine:
uv sync --extra indigo
# optional perception kernel (not layout):
uv sync --extra chematic
```

Export JSON Schema (committed under `schema/`):

```bash
uv run xpict-export-schema
```

## License

MIT

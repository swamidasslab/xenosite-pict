# xenosite-pict

Declarative, publication-quality small-molecule depiction for Python and the web.

Import path: `xenosite.pict`

## Status

Scaffold in progress. Language-neutral JSON contracts (Pydantic → generated JSON Schema) with Python and `js/` engines.

**Depiction:** Hard — follow CDK / RDKit / Indigo / CoordGen; don’t invent. Own SVG (skeleton → offsets → stereo). The product goal is a **native** depictor that is *demonstrably* good enough on a hard-case gallery.

**Chem stack (likely):** Chematic (or similar) for **perception** (aromaticity, SSSR, stereo — small Rust); **our** drawing. Chematic depict coords are not good enough. If we settle there, shipping depiction as a **Rust** crate (Py + WASM) is the natural fit — prove the draw/layout model in Python first.

**Layout coords (transitional):** **Indigo only** while native matures. No multi-backend ladder (RDKit / Open Babel / Chematic-as-layout are out of the product path). Native stub remains for tests without Indigo.

**Multi-molecule diagrams:** ELK via **jsrun** (embedded V8 + vendored elkjs) — no Node required. Grid/row fallback if ELK fails.

**Outputs:** SVG (default); HTML with embedded SVG for responsive pages.

## Quick start

```python
from xenosite.pict import Pict, render

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
uv run xenosite-pict-export-schema
```

## License

MIT

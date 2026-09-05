# xenosite-pict

Declarative, publication-quality small-molecule depiction for Python and the web.

Import path: `xenosite.pict`

## Status

Scaffold in progress. Language-neutral JSON contracts (Pydantic → generated JSON Schema) with Python and `js/` engines.

**Depiction:** Hard — follow CDK / RDKit / Indigo / CoordGen; don’t invent. Own SVG (skeleton → offsets → stereo). If **native** layout is *demonstrably* good enough on hard cases, multi-backend layout largely goes away (see `docs/layout-notes.md`).

**Chem stack (likely):** Chematic (or similar) for **perception** (aromaticity, SSSR, stereo flags — small Rust package); **our** drawing. Chematic depict coords are not good enough — do not ship them as the picture.

**Layout coords (transitional):** Indigo → RDKit → Open Babel → Chematic-coords-last-resort → native stub.

**Multi-molecule diagrams:** ELK (JAR + V8 in Python; elkjs on web) — bridge stubbed; grid/row fallback works now.

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
# optional chem engines:
uv sync --extra indigo
uv sync --extra rdkit
uv sync --extra openbabel
```

Export JSON Schema (committed under `schema/`):

```bash
uv run xenosite-pict-export-schema
```

## License

MIT

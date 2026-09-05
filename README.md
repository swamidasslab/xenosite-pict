# xenosite-pict

Declarative, publication-quality small-molecule depiction for Python and the web.

Import path: `xenosite.pict`

## Status

Scaffold in progress. Language-neutral JSON contracts (Pydantic → generated JSON Schema) with Python and `js/` engines.

**Depiction:** Hard problem — follow CDK / RDKit / Indigo / CoordGen, don’t invent. Own the SVG draw path (skeleton → offsets → stereo). Optional chem engines supply coords while **native** layout matures; if native is *demonstrably* good enough on hard cases, multi-backend layout becomes largely unnecessary (see `docs/layout-notes.md`).

**Layout (transitional):** Indigo → RDKit → Open Babel → Chematic → native stub.

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

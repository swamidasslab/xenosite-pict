# xpict

Declarative, publication-quality small-molecule depiction for Python and the web.

Import path: `xpict`

## Status

Scaffold in progress. Language-neutral JSON contracts (Pydantic → generated JSON Schema) with Python and `js/` engines.

**Depiction:** Hard — follow CDK / RDKit / Indigo / CoordGen; don’t invent. Own SVG (skeleton → offsets → stereo). Long-term goal: a **native** depictor proven on a hard-case gallery.

**Ship path:** client-side on [xenosite.org](https://xenosite.org) via WASM —
[`docs/migration-xenosite.md`](docs/migration-xenosite.md). Package for the site:
[`@swamidasslab/xpict`](docs/npm-xenosite.md) on GitHub Packages.

**Focus now — RDKit at the language edges:**
- **2D coords** via Python `rdkit` / JS `@rdkit/rdkit` (`backend="rdkit"`, auto-picked when installed)
- **Alignment** via the same RDKit packages (template depict); maps/coords then into Rust for shared math
- **No RDKit inside `xpict-core` / WASM**

**Alternate:** **Indigo** layout (`backend="indigo"`) + **fake/rigid align** in Rust (no RDKit template). Chematic is out.

**Shared Rust core:** fonts/geometry/draw helpers — see [`docs/bindings.md`](docs/bindings.md).

```bash
./scripts/build_bindings.sh all
uv sync --extra rdkit
pytest tests/test_native_rust.py tests/test_rdkit_backend.py
cd js && npm test
```

**Multi-molecule diagrams:** ELK via native **`elkrs`** (`_native.elk_layout_json`).
Grid/row if ELK fails. **jsrun / vendored elkjs removed.**

**Core deps:** `pydantic` + Rust extension (`geom`/`font`/`elk`). RDKit / Indigo stay
**language-edge extras**.

**Outputs:** SVG (default); HTML with embedded SVG for responsive pages.

## Quick start

```python
from xpict import Pict, render

# prefers rdkit when installed, else native
svg = Pict().render({"molecules": [{"smiles": "CCO"}]})
svg = Pict(backend="rdkit").render({"molecules": [{"smiles": "c1ccccc1O"}]})
# alternate layout + rigid align only:
svg = Pict(backend="indigo").render({"molecules": [{"smiles": "CCO"}]})
```

Input formats per molecule: `smiles`, `cxsmiles`, `esmiles`, or `molfile`.

Backend is runtime config (not in the JSON document). Unsupported options emit `PictBackendWarning`.

## Install

```bash
uv sync
uv sync --extra rdkit    # 2D layout + template alignment (focus)
uv sync --extra indigo   # alternate layout
uv sync --extra all
```

Export JSON Schema (committed under `schema/`):

```bash
uv run xpict-export-schema
```

## License

MIT

# Contracts

Pydantic models in `xenosite.pict.contracts` are the **source of truth**. Build/export writes portable JSON Schema to `schema/`:

- `pict.schema.json` — declarative input (`PictSpec`)
- `layout.schema.json` — backend layout result
- `scene.schema.json` — drawable scene graph

```bash
uv run xenosite-pict-export-schema
```

`js/` and any future engine should validate against these schemas. Engines are interchangeable implementations.

## Runtime vs document

- **In the document:** molecules (smiles / cxsmiles / esmiles / molfile), marks, shade, diagram kind, ELK options, sizes.
- **Runtime only:** `backend`, output `format` (`svg` | `html`).

## Partial backend support

If a backend cannot honor an option, it must `warnings.warn(..., PictBackendWarning)` and continue best-effort.

## Composition with Vega / HoloViews

Emit SVG/HTML fragments; compose as sibling views. Vega has no molecule-mark plugin API — do not embed as a Vega mark type.

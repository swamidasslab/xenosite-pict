# Contracts

## Shipped public API (language clients)

The **product surface** today is not a full nested figure document. It is:

1. `mol(source)` → input handle  
2. `render(mol, opts?)` → `Rendered` (`scene`, coords, `frame_molblock`, …)  
3. `toSvg(scene)` → SVG  
4. **Batch stub:** `depict({ molecules: MolSpec[] })` → `Rendered[]`  

`MolSpec` fields match current render options (`smiles` / `source` / `molfile` /
`cxsmiles`, marks, shade, color, star labels, `align_to` as an **index** into
earlier entries). This stub is the seam to grow toward full `PictSpec`.

Scene JSON (`xpict-core::Scene` / `scene.schema.json`) remains the paint ABI
between layout and SVG.

## Future declarative tree (`PictSpec`)

Pydantic models in `xpict.contracts` describe the **long-term** nested input
(`group` / `grid` / `reaction` / …). Build/export writes portable JSON Schema
to `schema/`:

- `xpict.schema.json` — full declarative input (`PictSpec`) — **not** the MVP ship surface  
- `layout.schema.json` — backend layout result  
- `scene.schema.json` — drawable scene graph  

```bash
uv run xpict-export-schema
```

Python `Pict().render(spec)` still exercises that path for lab/CI. Public
package docs and xenosite should lead with `mol` / `render` / `toSvg` /
`depict`.

## Runtime vs document

- **In a mol / MolSpec:** structure string, marks, shade, color, star labels, align index.  
- **Runtime only:** layout backend choice (RDKit vs Indigo vs native), output format where applicable.

## Partial backend support

If a backend cannot honor an option, it must `warnings.warn(..., PictBackendWarning)` and continue best-effort.

## Composition with Vega / HoloViews

Emit SVG/HTML fragments; compose as sibling views. Vega has no molecule-mark plugin API — do not embed as a Vega mark type.

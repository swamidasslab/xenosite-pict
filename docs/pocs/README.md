# Derisk POCs (schema alpha)

Schema stays alpha. These POCs stress the hard parts so learnings can trickle back into `PictSpec` / layout / scene contracts.

| POC | Hard question | Entry |
| --- | --- | --- |
| **A — Own SVG from chem coords** | Can Indigo layouts feed a publication-quality SVG we own (no SVG mutation)? | `uv run python scripts/run_pocs.py svg` |
| **B — Marks + score shading** | Do atom/bond/substructure marks and plot-dot shading work on real layouts? | `uv run python scripts/run_pocs.py shade` |
| **C — ELK multi-mol** | Can elkjs place network/reaction viewports via **jsrun** (no Node)? | `uv run python scripts/run_pocs.py elk` |

Artifacts land in `artifacts/pocs/` (gitignored samples) or `/opt/cursor/artifacts/` when run in cloud.

**Visual canvas:** open `docs/pocs/gallery.html` (or the copy under `/opt/cursor/artifacts/gallery.html`) after regenerating. The page inlines the POC SVGs so you can refresh as drawing quality improves.

```bash
uv run python scripts/run_pocs.py
# then open docs/pocs/gallery.html
```

Schema feedback from these runs: [`schema-feedback.md`](schema-feedback.md).

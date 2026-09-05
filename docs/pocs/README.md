# Derisk POCs (schema alpha)

Schema stays alpha. These POCs stress the hard parts so learnings can trickle
back into `PictSpec` / layout / scene contracts.

| POC | Hard question | Entry |
| --- | --- | --- |
| **A — Own SVG** | Can Indigo layouts feed a publication SVG we own (no SVG mutation)? | `uv run python scripts/run_pocs.py svg` |
| **B — Marks + shade** | Do atom/bond/substructure marks and plot-dot shading work on real layouts? | `uv run python scripts/run_pocs.py shade` |
| **C — ELK multi-mol** | Can elkjs place network viewports via **jsrun** (no Node)? | `uv run python scripts/run_pocs.py elk` |
| **D — Full pipeline** | Hard grid + stereo wedges + SoM shade + ELK pathway in one regeneratable set? | `uv run python scripts/run_pocs.py pipeline` |
| **E — Native vs Indigo** | Does experimental native ring/chain layout look plausible beside Indigo? | `uv run python scripts/run_pocs.py native` |

Artifacts land in `artifacts/pocs/` (gitignored) or `/opt/cursor/artifacts/` in cloud.

**Visual canvas:** open `docs/pocs/gallery.html` (or `/opt/cursor/artifacts/gallery.html`)
after regenerating. The page inlines the POC SVGs so you can refresh as drawing
quality improves.

```bash
uv run python scripts/run_pocs.py          # all POCs + gallery
uv run python scripts/run_pocs.py pipeline # hard molecules + stereo + SoM + pathway
uv run python scripts/run_pocs.py native   # native vs indigo grids
# then open docs/pocs/gallery.html
```

Schema feedback from these runs: [`schema-feedback.md`](schema-feedback.md).

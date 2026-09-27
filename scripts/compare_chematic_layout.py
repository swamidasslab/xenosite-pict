#!/usr/bin/env python3
"""Side-by-side RDKit vs chematic 2D layout for five real drugs.

Host flag only (``chematic_layout=True`` / ``XPICT_CHEMATIC_LAYOUT``) — not
DepictSpec schema. Writes SVG pairs + an HTML index under ``--out``.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from xpict import mol, render, to_svg
from xpict.client import MolRenderOptions

DRUGS: list[tuple[str, str]] = [
    ("aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
    ("ibuprofen", "CC(C)Cc1ccc(cc1)C(C)C(=O)O"),
    ("caffeine", "CN1C=NC2=C1C(=O)N(C(=O)N2C)C"),
    ("fluoxetine", "CNCCC(c1ccc(C(F)(F)F)cc1)c2ccccc2"),
    ("morphine", "CN1CC[C@]23c4c5ccc(O)c4O[C@H]2[C@@H](O)C=C[C@H]3[C@H]1C5"),
]


def _panel(label: str, svg: str, width: float, height: float) -> str:
    return (
        f'<figure class="panel">'
        f"<figcaption>{label}</figcaption>"
        f'<div class="svg-wrap" style="width:{width:.0f}px;height:{height:.0f}px">'
        f"{svg}</div></figure>"
    )


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--out",
        type=Path,
        default=Path("demo/chematic-layout-compare"),
        help="output directory for SVGs + index.html",
    )
    args = ap.parse_args()
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)

    rows: list[str] = []
    for name, smiles in DRUGS:
        rd = render(mol(smiles))
        ch = render(mol(smiles), MolRenderOptions(chematic_layout=True))
        rd_svg = to_svg(rd.scene)
        ch_svg = to_svg(ch.scene)
        (out / f"{name}-rdkit.svg").write_text(rd_svg, encoding="utf-8")
        (out / f"{name}-chematic.svg").write_text(ch_svg, encoding="utf-8")
        rows.append(
            f'<section class="drug"><h2>{name}</h2>'
            f'<code class="smi">{smiles}</code>'
            f'<div class="row">'
            f"{_panel('RDKit Depictor', rd_svg, rd.width, rd.height)}"
            f"{_panel('chematic compute_layout', ch_svg, ch.width, ch.height)}"
            f"</div></section>"
        )
        print(f"{name}: rdkit={rd.width:.0f}x{rd.height:.0f} chematic={ch.width:.0f}x{ch.height:.0f}")

    html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<title>RDKit vs chematic 2D layout</title>
<style>
  body {{ font-family: "Iowan Old Style", "Palatino Linotype", Palatino, serif;
         margin: 2rem; background: #f7f4ef; color: #1a1a1a; }}
  h1 {{ font-weight: 600; letter-spacing: -0.02em; }}
  h2 {{ margin: 0 0 0.25rem; text-transform: capitalize; }}
  .smi {{ display: block; font-size: 0.85rem; color: #555; margin-bottom: 0.75rem;
          word-break: break-all; }}
  .drug {{ margin-bottom: 2.5rem; padding-bottom: 1.5rem;
           border-bottom: 1px solid #d9d2c5; }}
  .row {{ display: flex; flex-wrap: wrap; gap: 1.5rem; align-items: flex-start; }}
  .panel {{ margin: 0; }}
  .panel figcaption {{ font-size: 0.9rem; color: #444; margin-bottom: 0.35rem; }}
  .svg-wrap {{ background: #fff; border: 1px solid #e2dcd0; padding: 0.5rem; }}
  .svg-wrap svg {{ display: block; max-width: 100%; height: auto; }}
  .note {{ max-width: 42rem; line-height: 1.45; color: #333; }}
</style>
</head>
<body>
<h1>RDKit vs chematic coords</h1>
<p class="note">Same xpict paint path; only the 2D layout engine differs.
Chematic is opted in with host flag <code>chematic_layout=True</code>
(not DepictSpec schema). Bond length normalized to house scale.</p>
{"".join(rows)}
</body>
</html>
"""
    (out / "index.html").write_text(html, encoding="utf-8")
    print(f"wrote {out / 'index.html'}")


if __name__ == "__main__":
    main()

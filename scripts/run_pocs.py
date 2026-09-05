#!/usr/bin/env python3
"""Run derisk POCs and write SVG artifacts.

Usage:
  uv run python scripts/run_pocs.py [all|svg|shade|elk]
"""

from __future__ import annotations

import json
import sys
import warnings
from pathlib import Path

from xenosite.pict import Pict, render
from xenosite.pict.contracts.spec import PictSpec
from xenosite.pict.diagram.elk import elk_graph, layout_diagram
from xenosite.pict.warnings import PictBackendWarning

ROOT = Path(__file__).resolve().parents[1]
OUT_DIRS = [
    Path("/opt/cursor/artifacts"),
    ROOT / "artifacts" / "pocs",
]


def _out_dir() -> Path:
    for d in OUT_DIRS:
        try:
            d.mkdir(parents=True, exist_ok=True)
            probe = d / ".write_probe"
            probe.write_text("ok", encoding="utf-8")
            probe.unlink(missing_ok=True)
            return d
        except OSError:
            continue
    d = ROOT / "artifacts" / "pocs"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _pick_backend() -> str:
    try:
        Pict(backend="indigo").layout({"molecules": [{"smiles": "C"}]})
        return "indigo"
    except Exception:
        return "native"


def poc_svg(out: Path, backend: str) -> Path:
    """POC A: chem-engine coords → own SVG."""
    spec = {
        "molecules": [
            {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O", "title": "aspirin"},
            {"id": "indole", "smiles": "c1ccc2[nH]ccc2c1", "title": "indole"},
        ],
        "diagram": {"kind": "grid", "columns": 2},
    }
    svg = render(spec, backend=backend)
    path = out / f"poc-a-own-svg-{backend}.svg"
    path.write_text(svg, encoding="utf-8")
    return path


def poc_shade(out: Path, backend: str) -> Path:
    """POC B: marks + plot-dot shading on a real layout."""
    pict = Pict(backend=backend)
    base = {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O"}
    layout = pict.layout({"molecules": [base]}).molecules[0]
    n = len(layout.atoms)
    shade = [0.1] * n
    for i, a in enumerate(layout.atoms):
        if a.element == "O":
            shade[i] = 0.95
        elif a.element == "C" and a.label is None:
            shade[i] = 0.35
    ringish = [i for i, a in enumerate(layout.atoms) if a.element == "C"][:6]
    oxygens = [i for i, a in enumerate(layout.atoms) if a.element == "O"]
    spec = {
        "molecules": [
            {
                **base,
                "shade": {"atoms": shade, "vmin": 0.0, "vmax": 1.0},
                "marks": [
                    {"kind": "atoms", "atoms": oxygens, "color": "#0a7"},
                    {
                        "kind": "substructure",
                        "atoms": ringish,
                        "color": "#36c",
                        "label": "ring",
                    },
                ],
            }
        ]
    }
    svg = pict.render(spec)
    path = out / f"poc-b-marks-shade-{backend}.svg"
    path.write_text(svg, encoding="utf-8")
    (out / f"poc-b-marks-shade-{backend}.json").write_text(
        json.dumps(
            {"backend": backend, "n_atoms": n, "shade": shade, "ringish": ringish},
            indent=2,
        ),
        encoding="utf-8",
    )
    return path


def poc_elk(out: Path, backend: str) -> Path:
    """POC C: network diagram via elkjs bridge (row fallback if Node/elkjs missing)."""
    spec_dict = {
        "molecules": [
            {"id": "A", "smiles": "CCO"},
            {"id": "B", "smiles": "CC=O"},
            {"id": "C", "smiles": "CC(=O)O"},
            {"id": "D", "smiles": "c1ccccc1"},
        ],
        "diagram": {
            "kind": "network",
            "edges": [
                {"source": "A", "target": "B"},
                {"source": "B", "target": "C"},
                {"source": "A", "target": "D"},
            ],
            "elk_options": {"elk.direction": "DOWN"},
        },
    }
    doc = PictSpec.model_validate(spec_dict)
    pict = Pict(backend=backend)
    layouts = pict.layout(doc).molecules
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        positions = layout_diagram(layouts, doc)
        svg = pict.render(doc)
    (out / "poc-c-elk-graph.json").write_text(
        json.dumps(elk_graph(layouts, doc), indent=2), encoding="utf-8"
    )
    (out / "poc-c-elk-positions.json").write_text(
        json.dumps(
            {
                "positions": positions,
                "warnings": [
                    str(w.message)
                    for w in caught
                    if issubclass(w.category, PictBackendWarning)
                ],
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    path = out / f"poc-c-network-{backend}.svg"
    path.write_text(svg, encoding="utf-8")
    return path


def main(argv: list[str]) -> int:
    which = (argv[1] if len(argv) > 1 else "all").lower()
    out = _out_dir()
    backend = _pick_backend()
    print(f"backend={backend} out={out}")
    runners = {"svg": poc_svg, "shade": poc_shade, "elk": poc_elk}
    keys = list(runners) if which == "all" else [which]
    written: list[Path] = []
    for key in keys:
        if key not in runners:
            print(f"unknown poc: {key}", file=sys.stderr)
            return 2
        path = runners[key](out, backend)
        written.append(path)
        print(f"wrote {path}")
    gallery = write_gallery(out, backend, written if which != "all" else None)
    print(f"gallery {gallery}")
    # Also refresh the checked-in docs gallery when running from repo root.
    docs_gallery = ROOT / "docs" / "pocs" / "gallery.html"
    if docs_gallery.parent.is_dir() and which == "all":
        docs_gallery.write_text(gallery.read_text(encoding="utf-8"), encoding="utf-8")
        print(f"gallery {docs_gallery}")
    return 0


def write_gallery(out: Path, backend: str, only: list[Path] | None = None) -> Path:
    """Build a self-contained HTML page embedding POC SVGs for visual iteration."""
    cards = [
        (
            "A — Own SVG",
            "Indigo/RDKit coords → our scene → SVG (no engine SVG mutation).",
            out / f"poc-a-own-svg-{backend}.svg",
        ),
        (
            "B — Marks + plot-dot shade",
            "Per-atom scores as concentric plot-dots, atom marks, substructure hull.",
            out / f"poc-b-marks-shade-{backend}.svg",
        ),
        (
            "C — ELK network",
            "Multi-mol network placement via elkjs Node bridge (row fallback if missing).",
            out / f"poc-c-network-{backend}.svg",
        ),
    ]
    if only is not None:
        only_names = {p.name for p in only}
        cards = [c for c in cards if c[2].name in only_names]

    sections: list[str] = []
    for title, blurb, path in cards:
        if not path.is_file():
            body = f"<p class='missing'>Missing artifact: {path.name}</p>"
        else:
            svg = path.read_text(encoding="utf-8")
            if svg.startswith("<?xml"):
                svg = svg.split("\n", 1)[1]
            body = f'<div class="frame">{svg}</div>'
        sections.append(
            f"""
<article class="card">
  <header>
    <h2>{title}</h2>
    <p>{blurb}</p>
    <p class="meta"><code>{path.name}</code> · backend=<code>{backend}</code></p>
  </header>
  {body}
</article>"""
        )

    html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>xenosite.pict · derisk POC gallery</title>
<style>
  :root {{
    color-scheme: light;
    --bg: #e8e6e1;
    --card: #ffffff;
    --ink: #1a1a1a;
    --muted: #5c5c5c;
    --line: #d0cdc6;
    --accent: #0b6e4f;
  }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0;
    font-family: "IBM Plex Sans", "Source Sans 3", system-ui, sans-serif;
    background:
      radial-gradient(circle at 10% 0%, #f4f1ea 0%, transparent 45%),
      radial-gradient(circle at 90% 10%, #dde8e2 0%, transparent 40%),
      var(--bg);
    color: var(--ink);
    line-height: 1.45;
  }}
  main {{
    max-width: 1100px;
    margin: 0 auto;
    padding: 2rem 1.25rem 4rem;
  }}
  h1 {{
    font-family: "IBM Plex Serif", "Source Serif 4", Georgia, serif;
    font-weight: 600;
    font-size: clamp(1.8rem, 3vw, 2.4rem);
    margin: 0 0 0.35rem;
    letter-spacing: -0.02em;
  }}
  .lede {{
    color: var(--muted);
    max-width: 42rem;
    margin: 0 0 1.75rem;
  }}
  .card {{
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 1.1rem 1.1rem 1.25rem;
    margin: 0 0 1.25rem;
    box-shadow: 0 1px 0 rgba(0,0,0,0.03);
  }}
  .card h2 {{
    font-size: 1.15rem;
    margin: 0 0 0.25rem;
    color: var(--accent);
  }}
  .card header p {{ margin: 0.15rem 0; }}
  .meta {{ font-size: 0.85rem; color: var(--muted); }}
  .frame {{
    margin-top: 0.9rem;
    background: #fff;
    border: 1px solid var(--line);
    border-radius: 2px;
    overflow: auto;
    padding: 0.75rem;
  }}
  .frame svg {{
    display: block;
    max-width: 100%;
    height: auto;
    background: #fff !important;
  }}
  .missing {{ color: #a33; }}
  footer {{
    margin-top: 2rem;
    color: var(--muted);
    font-size: 0.9rem;
  }}
  code {{ font-family: "IBM Plex Mono", ui-monospace, monospace; font-size: 0.92em; }}
</style>
</head>
<body>
<main>
  <h1>xenosite.pict derisk POCs</h1>
  <p class="lede">
    Visual canvas for the hard paths while the schema stays alpha.
    Regenerate with <code>uv run python scripts/run_pocs.py</code>.
    SVGs always carry an opaque white background so dark IDE themes do not invert bonds.
  </p>
  {"".join(sections)}
  <footer>Schema feedback: <code>docs/pocs/schema-feedback.md</code></footer>
</main>
</body>
</html>
"""
    path = out / "gallery.html"
    path.write_text(html, encoding="utf-8")
    return path


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

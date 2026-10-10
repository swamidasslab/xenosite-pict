#!/usr/bin/env python3
"""Run derisk POCs via native Rust ``depict`` and write SVG artifacts.

Usage:
  uv run python scripts/run_pocs.py [all|svg|shade|elk|pipeline|compare]
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

from xpict import Pict, depict, render
from xpict.draw.svg import svg_to_img_tag

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


def _grid(mols: list[dict], *, columns: int) -> dict:
    """Legacy-shaped grid → live DepictSpec via ``Pict`` lift."""
    return {
        "molecules": mols,
        "diagram": {"kind": "grid", "columns": columns},
    }


def poc_svg(out: Path) -> Path:
    """POC A: native layout → SVG."""
    svg = render(
        _grid(
            [
                {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O", "title": "aspirin"},
                {"id": "indole", "smiles": "c1ccc2[nH]ccc2c1", "title": "indole"},
            ],
            columns=2,
        )
    )
    path = out / "poc-a-own-svg.svg"
    path.write_text(svg, encoding="utf-8")
    return path


def poc_shade(out: Path) -> Path:
    """POC B: shade scores on a real layout."""
    pict = Pict()
    base = {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O"}
    layout = pict.layout({"molecules": [base]})[0]
    n = len(layout.atoms)
    shade = [0.1] * n
    for i, a in enumerate(layout.atoms):
        if a.element == "O":
            shade[i] = 0.95
        elif a.element == "C" and a.label is None:
            shade[i] = 0.35
    svg = pict.render(
        {
            "molecules": [
                {
                    **base,
                    "shade": {"atoms": shade, "vmin": 0.0, "vmax": 1.0},
                }
            ]
        }
    )
    path = out / "poc-b-marks-shade.svg"
    path.write_text(svg, encoding="utf-8")
    (out / "poc-b-marks-shade.json").write_text(
        json.dumps({"n_atoms": n, "shade": shade}, indent=2),
        encoding="utf-8",
    )
    return path


def poc_elk(out: Path) -> Path:
    """POC C: network diagram via Rust compose_scheme (ELK)."""
    spec = {
        "type": "reaction_scheme",
        "children": [
            {"type": "mol", "id": "A", "smiles": "CCO"},
            {"type": "mol", "id": "B", "smiles": "CC=O"},
            {"type": "mol", "id": "C", "smiles": "CC(=O)O"},
            {"type": "mol", "id": "D", "smiles": "c1ccccc1"},
            {"type": "edge", "sources": "A", "targets": "B"},
            {"type": "edge", "sources": "B", "targets": "C"},
            {"type": "edge", "sources": "A", "targets": "D"},
        ],
        "layout": {"direction": "down"},
    }
    rows = depict(spec)
    scene = rows[0].scene
    positions = [(vp.id, vp.x, vp.y) for vp in scene.viewports]
    (out / "poc-c-elk-positions.json").write_text(
        json.dumps({"positions": positions}, indent=2),
        encoding="utf-8",
    )
    path = out / "poc-c-network.svg"
    path.write_text(render(spec), encoding="utf-8")
    return path


def poc_pipeline(out: Path) -> Path:
    """POC D: hard molecules, stereo, shade, pathway — native Rust only."""
    pict = Pict()

    hard = _grid(
        [
            {"id": "caffeine", "smiles": "CN1C=NC2=C1C(=O)N(C(=O)N2C)C", "title": "caffeine"},
            {"id": "indole", "smiles": "c1ccc2[nH]ccc2c1", "title": "indole"},
            {"id": "norbornane", "smiles": "C1CC2CCC1C2", "title": "norbornane"},
            {"id": "naphthalene", "smiles": "c1ccc2ccccc2c1", "title": "naphthalene"},
            {"id": "nitrobenzene", "smiles": "c1ccc(cc1)[N+](=O)[O-]", "title": "nitrobenzene"},
            {"id": "proline", "smiles": "O=C(O)[C@@H]1CCCN1", "title": "L-proline"},
        ],
        columns=3,
    )
    hard_path = out / "poc-d1-hard-grid.svg"
    hard_path.write_text(pict.render(hard), encoding="utf-8")

    stereo_path = out / "poc-d2-stereo.svg"
    stereo_path.write_text(
        pict.render(
            {"molecules": [{"id": "alanine", "smiles": "C[C@H](N)C(=O)O", "title": "L-alanine"}]}
        ),
        encoding="utf-8",
    )

    asa = {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O", "title": "aspirin SoM"}
    layout = pict.layout({"molecules": [asa]})[0]
    n = len(layout.atoms)
    shade = [0.05] * n
    for i, a in enumerate(layout.atoms):
        if a.element == "O":
            shade[i] = 0.95
        elif a.label is None and a.element == "C":
            shade[i] = 0.4
    som_path = out / "poc-d3-som-shade.svg"
    som_path.write_text(
        pict.render(
            {
                "molecules": [
                    {**asa, "shade": {"atoms": shade, "vmin": 0.0, "vmax": 1.0}}
                ]
            }
        ),
        encoding="utf-8",
    )

    pathway = {
        "molecules": [
            {"id": "etoh", "smiles": "CCO", "title": "ethanol"},
            {"id": "ach", "smiles": "CC=O", "title": "acetaldehyde"},
            {"id": "acetate", "smiles": "CC(=O)O", "title": "acetate"},
            {"id": "asa2", "smiles": "CC(=O)Oc1ccccc1C(=O)O", "title": "aspirin"},
            {"id": "sal", "smiles": "O=C(O)c1ccccc1O", "title": "salicylate"},
        ],
        "diagram": {
            "kind": "reaction",
            "edges": [
                {
                    "source": "etoh",
                    "target": "ach",
                    "label": "ADH",
                    "arrow": "forward",
                    "role": "enzyme",
                },
                {
                    "source": "ach",
                    "target": "acetate",
                    "label": "ALDH",
                    "arrow": "forward",
                    "role": "enzyme",
                },
                {
                    "source": "asa2",
                    "target": "sal",
                    "label": "CES",
                    "arrow": "forward",
                    "color": "#064",
                    "role": "enzyme",
                },
            ],
        },
    }
    path_path = out / "poc-d4-pathway.svg"
    path_path.write_text(pict.render(pathway), encoding="utf-8")

    branched = {
        "molecules": [
            {"id": "etoh", "smiles": "CCO", "title": "ethanol"},
            {"id": "ach", "smiles": "CC=O", "title": "acetaldehyde"},
            {"id": "acetate", "smiles": "CC(=O)O", "title": "acetate"},
            {"id": "phh", "smiles": "c1ccccc1", "title": "benzene"},
            {"id": "phenol", "smiles": "c1ccccc1O", "title": "phenol"},
            {"id": "catechol", "smiles": "c1ccc(O)c(O)c1", "title": "catechol"},
            {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O", "title": "aspirin"},
            {"id": "sal", "smiles": "O=C(O)c1ccccc1O", "title": "salicylate"},
        ],
        "diagram": {
            "kind": "reaction",
            "edges": [
                {"source": "etoh", "target": "ach", "label": "ADH"},
                {"source": "ach", "target": "acetate", "label": "ALDH"},
                {"source": "phh", "target": "phenol", "label": "CYP2E1"},
                {"source": "phenol", "target": "catechol", "label": "CYP"},
                {
                    "source": "ach",
                    "target": "phenol",
                    "label": "conj?",
                    "dashed": True,
                    "arrow": "line",
                    "color": "#888",
                },
                {"source": "asa", "target": "sal", "label": "CES", "color": "#064"},
                {
                    "source": "sal",
                    "target": "catechol",
                    "label": "decarb",
                    "arrow": "open",
                    "color": "#a40",
                },
            ],
        },
    }
    branch_path = out / "poc-d5-branched.svg"
    branch_path.write_text(pict.render(branched), encoding="utf-8")

    (out / "poc-d-pipeline.json").write_text(
        json.dumps(
            {
                "engine": "native",
                "artifacts": [
                    hard_path.name,
                    stereo_path.name,
                    som_path.name,
                    path_path.name,
                    branch_path.name,
                ],
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    return hard_path


def poc_compare(out: Path) -> Path:
    """POC E: layout grid on shared SMILES."""
    cases = [
        ("benzene", "c1ccccc1"),
        ("naphthalene", "c1ccc2ccccc2c1"),
        ("anthracene", "c1ccc2cc3ccccc3cc2c1"),
        ("hexane", "CCCCCC"),
        ("branched", "CC(C)CCCC"),
        ("phenol", "c1ccc(cc1)O"),
        ("aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
        ("norbornane", "C1CC2CCC1C2"),
        ("chiral", "C[C@H](O)Cl"),
        ("trans", r"F/C=C/F"),
        ("cis", r"F/C=C\F"),
        ("pyrrole", "c1ccc[nH]1"),
        ("hexylcyclohexane", "C1CCCCC1CCOCCCCCC"),
    ]
    spec = _grid(
        [{"id": f"p-{cid}", "smiles": smi, "title": cid} for cid, smi in cases],
        columns=3,
    )
    primary_path = out / "poc-e-grid.svg"
    primary_path.write_text(render(spec), encoding="utf-8")
    (out / "poc-e-compare.json").write_text(
        json.dumps(
            {
                "cases": [{"id": c, "smiles": s} for c, s in cases],
                "primary": primary_path.name,
                "engine": "native",
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    return primary_path


def write_gallery(out: Path, only: list[Path] | None = None) -> Path:
    """Build a self-contained HTML page embedding POC SVGs."""
    cards = [
        ("A — Own SVG", "Native CoordGen → scene → SVG.", out / "poc-a-own-svg.svg"),
        (
            "B — Shade",
            "Per-atom shade scores on aspirin.",
            out / "poc-b-marks-shade.svg",
        ),
        (
            "C — ELK network",
            "Multi-mol network via Rust compose_scheme.",
            out / "poc-c-network.svg",
        ),
        (
            "D1 — Hard-molecule grid",
            "Fused, bridged, charged, chiral set.",
            out / "poc-d1-hard-grid.svg",
        ),
        (
            "D2 — Stereo",
            "Chiral center via native layout.",
            out / "poc-d2-stereo.svg",
        ),
        (
            "D3 — SoM shade",
            "Aspirin shade annotation.",
            out / "poc-d3-som-shade.svg",
        ),
        (
            "D4 — Pathway (ELK)",
            "Ethanol oxidation + aspirin hydrolysis.",
            out / "poc-d4-pathway.svg",
        ),
        (
            "D5 — Branched scheme",
            "Forked network with overlay arrows.",
            out / "poc-d5-branched.svg",
        ),
        (
            "E — Layout grid",
            "Shared SMILES laid out with native CoordGen.",
            out / "poc-e-grid.svg",
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
            body = (
                f'<div class="frame"><div class="xpict-mol">'
                f"{svg_to_img_tag(svg, alt=title)}</div></div>"
            )
        sections.append(
            f"""
<article class="card">
  <header>
    <h2>{title}</h2>
    <p>{blurb}</p>
    <p class="meta"><code>{path.name}</code> · engine=<code>native</code></p>
  </header>
  {body}
</article>"""
        )

    html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>xpict · derisk POC gallery</title>
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
  .frame .xpict-mol {{
    position: relative;
    display: inline-block;
    max-width: 100%;
    line-height: 0;
  }}
  .frame img.xpict {{
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
  <h1>xpict derisk POCs</h1>
  <p class="lede">
    Native Rust <code>depict</code> only — no layout-backend switch.
    Regenerate with <code>uv run python scripts/run_pocs.py</code>.
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


def main(argv: list[str]) -> int:
    which = (argv[1] if len(argv) > 1 else "all").lower()
    out = _out_dir()
    print(f"engine=native out={out}")
    runners = {
        "svg": poc_svg,
        "shade": poc_shade,
        "elk": poc_elk,
        "pipeline": poc_pipeline,
        "native": poc_compare,
        "compare": poc_compare,
    }
    keys = list(runners) if which == "all" else [which]
    written: list[Path] = []
    for key in keys:
        if key not in runners:
            print(f"unknown poc: {key}", file=sys.stderr)
            return 2
        path = runners[key](out)
        written.append(path)
        print(f"wrote {path}")
        if key == "pipeline":
            for name in sorted(out.glob("poc-d*.svg")):
                if name not in written:
                    print(f"wrote {name}")
        if key in {"native", "compare"}:
            for name in sorted(out.glob("poc-e-*.svg")):
                print(f"wrote {name}")
    gallery = write_gallery(out, written if which != "all" else None)
    print(f"gallery {gallery}")
    docs_gallery = ROOT / "docs" / "pocs" / "gallery.html"
    if docs_gallery.parent.is_dir() and which == "all":
        docs_gallery.write_text(gallery.read_text(encoding="utf-8"), encoding="utf-8")
        print(f"gallery {docs_gallery}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

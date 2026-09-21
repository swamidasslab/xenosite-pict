#!/usr/bin/env python3
"""Run derisk POCs and write SVG artifacts.

Usage:
  uv run python scripts/run_pocs.py [all|svg|shade|elk|pipeline|native]
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
    """POC C: network diagram via elkjs in jsrun (row fallback if ELK fails)."""
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


def poc_pipeline(out: Path, backend: str) -> Path:
    """POC D: full pipeline collage — hard molecules, stereo, shade, pathway.

    One document that exercises Indigo layout → own skeleton/offset/wedge draw →
    marks/shade → ELK network in a single regeneratable artifact set.
    """
    pict = Pict(backend=backend)

    # D1 — non-trivial molecule grid (fused, charged, macro-ish, drug-like)
    hard = {
        "molecules": [
            {"id": "caffeine", "smiles": "CN1C=NC2=C1C(=O)N(C(=O)N2C)C", "title": "caffeine"},
            {"id": "indole", "smiles": "c1ccc2[nH]ccc2c1", "title": "indole"},
            {"id": "norbornane", "smiles": "C1CC2CCC1C2", "title": "norbornane"},
            {"id": "naphthalene", "smiles": "c1ccc2ccccc2c1", "title": "naphthalene"},
            {"id": "nitrobenzene", "smiles": "c1ccc(cc1)[N+](=O)[O-]", "title": "nitrobenzene"},
            {"id": "proline", "smiles": "O=C(O)[C@@H]1CCCN1", "title": "L-proline"},
        ],
        "diagram": {"kind": "grid", "columns": 3},
    }
    hard_svg = pict.render(hard)
    hard_path = out / f"poc-d1-hard-grid-{backend}.svg"
    hard_path.write_text(hard_svg, encoding="utf-8")

    # D2 — stereo wedges on a chiral center
    stereo_svg = pict.render(
        {"molecules": [{"id": "alanine", "smiles": "C[C@H](N)C(=O)O", "title": "L-alanine"}]}
    )
    stereo_path = out / f"poc-d2-stereo-{backend}.svg"
    stereo_path.write_text(stereo_svg, encoding="utf-8")

    # D3 — aspirin site-of-metabolism style shade + marks
    asa = {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O", "title": "aspirin SoM"}
    layout = pict.layout({"molecules": [asa]}).molecules[0]
    n = len(layout.atoms)
    shade = [0.05] * n
    for i, a in enumerate(layout.atoms):
        if a.element == "O":
            shade[i] = 0.95
        elif a.label is None and a.element == "C":
            shade[i] = 0.4
    oxygens = [i for i, a in enumerate(layout.atoms) if a.element == "O"]
    ring = [i for i, a in enumerate(layout.atoms) if a.element == "C"][:6]
    som_svg = pict.render(
        {
            "molecules": [
                {
                    **asa,
                    "shade": {"atoms": shade, "vmin": 0.0, "vmax": 1.0},
                    "marks": [
                        {"kind": "atoms", "atoms": oxygens, "color": "#0a7", "label": "O"},
                        {
                            "kind": "substructure",
                            "atoms": ring,
                            "color": "#36c",
                            "label": "aryl",
                        },
                    ],
                }
            ]
        }
    )
    som_path = out / f"poc-d3-som-shade-{backend}.svg"
    som_path.write_text(som_svg, encoding="utf-8")

    # D4 — mini ethanol→acetate + aromatic sink pathway (ELK via jsrun)
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
            "elk_options": {"elk.direction": "RIGHT"},
        },
    }
    path_svg = pict.render(pathway)
    path_path = out / f"poc-d4-pathway-{backend}.svg"
    path_path.write_text(path_svg, encoding="utf-8")

    # D5 — branched / cross-linked metabolic scheme (ELK orthogonal routes)
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
    branch_svg = pict.render(branched)
    branch_path = out / f"poc-d5-branched-{backend}.svg"
    branch_path.write_text(branch_svg, encoding="utf-8")

    manifest = {
        "backend": backend,
        "artifacts": [
            hard_path.name,
            stereo_path.name,
            som_path.name,
            path_path.name,
            branch_path.name,
        ],
        "notes": [
            "Indigo layout → own skeleton/offset/wedge SVG",
            "ELK pathway placement via jsrun+elkjs",
            "Branched scheme uses ELK orthogonal edge routes → overlay arrows",
            "Shade/marks on aspirin as SoM-style annotation",
        ],
    }
    (out / "poc-d-pipeline.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    # Return the hard grid as the primary path for gallery wiring.
    return hard_path


def poc_native(out: Path, backend: str) -> Path:
    """POC E: native vs Indigo side-by-side on shared SMILES (depictor progress)."""
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
    ]
    # Native grid
    native_spec = {
        "molecules": [
            {"id": f"n-{cid}", "smiles": smi, "title": f"{cid} (native)"}
            for cid, smi in cases
        ],
        "diagram": {"kind": "grid", "columns": 3},
    }
    native_svg = render(native_spec, backend="native")
    native_path = out / "poc-e-native-grid.svg"
    native_path.write_text(native_svg, encoding="utf-8")

    # Reference grid with transitional backend (indigo when available)
    ref_backend = backend if backend != "native" else "native"
    ref_spec = {
        "molecules": [
            {"id": f"r-{cid}", "smiles": smi, "title": f"{cid} ({ref_backend})"}
            for cid, smi in cases
        ],
        "diagram": {"kind": "grid", "columns": 3},
    }
    ref_svg = render(ref_spec, backend=ref_backend)
    ref_path = out / f"poc-e-ref-grid-{ref_backend}.svg"
    ref_path.write_text(ref_svg, encoding="utf-8")

    (out / "poc-e-compare.json").write_text(
        json.dumps(
            {
                "cases": [{"id": c, "smiles": s} for c, s in cases],
                "native": native_path.name,
                "reference": ref_path.name,
                "reference_backend": ref_backend,
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    return native_path


def main(argv: list[str]) -> int:
    which = (argv[1] if len(argv) > 1 else "all").lower()
    out = _out_dir()
    backend = _pick_backend()
    print(f"backend={backend} out={out}")
    runners = {
        "svg": poc_svg,
        "shade": poc_shade,
        "elk": poc_elk,
        "pipeline": poc_pipeline,
        "native": poc_native,
    }
    keys = list(runners) if which == "all" else [which]
    written: list[Path] = []
    for key in keys:
        if key not in runners:
            print(f"unknown poc: {key}", file=sys.stderr)
            return 2
        path = runners[key](out, backend)
        written.append(path)
        print(f"wrote {path}")
        # pipeline/native write multiple files — list extras
        if key == "pipeline":
            for name in sorted(out.glob(f"poc-d*-{backend}.svg")):
                if name not in written:
                    print(f"wrote {name}")
        if key == "native":
            for name in sorted(out.glob("poc-e-*.svg")):
                print(f"wrote {name}")
    gallery = write_gallery(out, backend, written if which != "all" else None)
    print(f"gallery {gallery}")
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
            "Indigo coords → our scene → SVG (no engine SVG mutation).",
            out / f"poc-a-own-svg-{backend}.svg",
        ),
        (
            "B — Marks + plot-dot shade",
            "Per-atom scores as concentric plot-dots, atom marks, substructure hull.",
            out / f"poc-b-marks-shade-{backend}.svg",
        ),
        (
            "C — ELK network",
            "Multi-mol network placement via elkjs inside jsrun (row fallback if ELK fails).",
            out / f"poc-c-network-{backend}.svg",
        ),
        (
            "D1 — Hard-molecule grid",
            "Full pipeline collage: fused, bridged, charged, chiral drug-like set.",
            out / f"poc-d1-hard-grid-{backend}.svg",
        ),
        (
            "D2 — Stereo wedges",
            "Chiral center with solid/hashed wedges from Indigo stereo → own draw.",
            out / f"poc-d2-stereo-{backend}.svg",
        ),
        (
            "D3 — SoM shade",
            "Aspirin with site-of-metabolism style shade + oxygen/aryl marks.",
            out / f"poc-d3-som-shade-{backend}.svg",
        ),
        (
            "D4 — Pathway (ELK)",
            "Ethanol oxidation + aspirin hydrolysis pathway laid out by jsrun+elkjs.",
            out / f"poc-d4-pathway-{backend}.svg",
        ),
        (
            "D5 — Branched scheme (ELK routes)",
            "Forked metabolic network with orthogonal ELK edge routes and styled arrows.",
            out / f"poc-d5-branched-{backend}.svg",
        ),
        (
            "E — Native depictor grid",
            "Experimental native layout (regular rings + 120° chains) on shared SMILES.",
            out / "poc-e-native-grid.svg",
        ),
        (
            "E — Indigo reference grid",
            "Same SMILES with Indigo transitional layout for side-by-side comparison.",
            out / f"poc-e-ref-grid-{backend}.svg",
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
    SVG backgrounds are clear by default so the host page shows through.
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

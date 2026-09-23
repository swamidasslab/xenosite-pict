#!/usr/bin/env python3
"""Generate SVG depiction examples for the MkDocs gallery.

Documents use the live nested subset of PictSpec (``type`` / ``children``).
"""

from __future__ import annotations

import re
from pathlib import Path

from xpict import render

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "assets" / "examples"


def _shade_vec(n: int, hot: dict[int, float], *, fill: float = 0.0) -> list[float]:
    """Per-atom shade vector of length ``n``; ``hot`` overrides selected indices."""
    zs = [fill] * n
    for i, z in hot.items():
        if 0 <= i < n:
            zs[i] = z
    return zs


# Realistic single-mol docs (nested subset). Shade vectors sized after a
# layout pass when needed — fixed lengths below match RDKit encounter order
# for these SMILES (verified in tests/test_gallery_examples.py).
CASES: dict[str, dict] = {
    "ethanol": {"type": "mol", "smiles": "CCO"},
    "benzene": {"type": "mol", "smiles": "c1ccccc1"},
    "phenol": {"type": "mol", "smiles": "c1ccccc1O"},
    "aspirin": {"type": "mol", "smiles": "CC(=O)Oc1ccccc1C(=O)O"},
    "caffeine": {"type": "mol", "smiles": "CN1C=NC2=C1C(=O)N(C(=O)N2C)C"},
    "glucose": {
        "type": "mol",
        "smiles": "OC[C@H]1OC(O)[C@H](O)[C@@H](O)[C@@H]1O",
    },
    "ibuprofen": {
        "type": "mol",
        "smiles": "CC(C)Cc1ccc(cc1)[C@@H](C)C(=O)O",
    },
    "penicillin_g": {
        "type": "mol",
        "smiles": "CC1(C)S[C@@H]2[C@H](NC(=O)Cc3ccccc3)C(=O)N2[C@H]1C(=O)O",
    },
    "markush": {
        "type": "mol",
        # Real ChemAxon CXSMILES atom alias (literal R1).
        "cxsmiles": "*c1ccccc1Cl |$R1;;;;;$|",
    },
    # Shade walkthrough examples (keep scores light so drawings stay readable).
    "aspirin_shade": {
        "type": "mol",
        "smiles": "CC(=O)Oc1ccccc1C(=O)O",
        "shade": {
            "atoms": [0.0, 0.0, 0.15, 0.0, 0.0, 0.0, 0.0, 0.55, 0.0, 0.0, 0.0, 0.9, 0.0],
            "vmin": 0.0,
            "vmax": 1.0,
        },
    },
    "phenol_shade": {
        "type": "mol",
        "smiles": "c1ccccc1O",
        "shade": {
            "atoms": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.95],
            "vmin": 0.0,
            "vmax": 1.0,
        },
    },
    "pyridine_shade": {
        "type": "mol",
        "smiles": "c1ccncc1",
        "shade": {
            "atoms": [0.1, 0.0, 0.0, 0.8, 0.0, 0.0],
            "vmin": 0.0,
            "vmax": 1.0,
        },
    },
    # Walkthrough / API docs (shown next to code samples).
    "benzene_color": {
        "type": "mol",
        "smiles": "c1ccccc1",
        "color": "#0b6e4f",
    },
    "toluene": {"type": "mol", "smiles": "Cc1ccccc1"},
    # Visual match for star_labels: ["$R_1$"] — Unicode alias in CX (real CXSMILES).
    "star_r1": {
        "type": "mol",
        "cxsmiles": "*C |$R₁;$|",
    },
    "ethanol_shade": {
        "type": "mol",
        "smiles": "CCO",
        "color": "#0b6e4f",
        "shade": {
            "atoms": [0.0, 0.2, 0.9],
            "vmin": 0.0,
            "vmax": 1.0,
        },
    },
}


def _render(doc: dict) -> str:
    try:
        return render(doc, backend="rdkit")
    except Exception:
        return render(doc, backend="native")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, doc in CASES.items():
        svg = _render(doc)
        path = OUT / f"{name}.svg"
        path.write_text(svg, encoding="utf-8")
        texts = re.findall(r'data-text="([^"]*)"', svg)
        print(f"wrote {path.relative_to(ROOT)} ({len(svg)} bytes) labels={texts[:6]}")


if __name__ == "__main__":
    main()

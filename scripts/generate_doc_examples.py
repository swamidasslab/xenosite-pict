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


def _sparse(n: int, hot: dict[int, float], *, fill: float = 0.0) -> list[float]:
    """Per-atom shade vector: mostly ``fill``, with a few hot indices."""
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
        # Braced markup inside CX — see docs/label-markup.md
        "cxsmiles": "*c1ccccc1Cl |$R_{1};;;;;$|",
    },
    "markush_json": {
        "type": "mol",
        "smiles": "*c1ccc(O)cc1",
        "rgroups": ["$R_1$"],
    },
    # Sparse shade: most zeros, a few hot atoms (SoM-style highlight).
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

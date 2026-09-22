#!/usr/bin/env python3
"""Generate SVG depiction examples for the MkDocs gallery."""

from __future__ import annotations

from pathlib import Path

from xpict import render

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "assets" / "examples"

CASES: dict[str, dict] = {
    "aspirin": {"molecules": [{"smiles": "CC(=O)Oc1ccccc1C(=O)O"}]},
    "caffeine": {"molecules": [{"smiles": "CN1C=NC2=C1C(=O)N(C(=O)N2C)C"}]},
    "ethanol": {"molecules": [{"smiles": "CCO"}]},
    "benzene": {"molecules": [{"smiles": "c1ccccc1"}]},
    "glucose": {
        "molecules": [{"smiles": "OC[C@H]1OC(O)[C@H](O)[C@@H](O)[C@@H]1O"}]
    },
    "markush": {"molecules": [{"cxsmiles": "*c1ccccc1Cl |$R1;;;;;$|"}]},
    "phenol_marks": {
        "molecules": [{"smiles": "c1ccccc1O", "marks": [{"atoms": [6]}]}],
    },
}


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, doc in CASES.items():
        try:
            svg = render(doc, backend="rdkit")
        except Exception:
            svg = render(doc, backend="native")
        path = OUT / f"{name}.svg"
        path.write_text(svg, encoding="utf-8")
        print(f"wrote {path.relative_to(ROOT)} ({len(svg)} bytes)")


if __name__ == "__main__":
    main()

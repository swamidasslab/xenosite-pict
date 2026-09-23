"""Gallery example regressions — markup, nested docs, shade."""

from __future__ import annotations

import re
from pathlib import Path

import pytest
from pydantic import ValidationError

from xpict import DepictSpec, render

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "docs" / "assets" / "examples"


def _texts(svg: str) -> list[str]:
    return re.findall(r'data-text="([^"]*)"', svg)


def _require_rdkit() -> None:
    pytest.importorskip("rdkit")


def test_gallery_markush_cx_r1():
    _require_rdkit()
    svg = render(
        {"type": "mol", "cxsmiles": "*c1ccccc1Cl |$R1;;;;;$|"},
        backend="rdkit",
    )
    texts = _texts(svg)
    assert "R1" in texts, f"expected literal R1 from CX alias, got {texts}"


def test_gallery_phenol_star_cx_r1():
    _require_rdkit()
    svg = render(
        {"type": "mol", "cxsmiles": "*c1ccc(O)cc1 |$R1;;;;;$|"},
        backend="rdkit",
    )
    texts = _texts(svg)
    assert "R1" in texts, f"expected literal R1 from CX alias, got {texts}"


def test_shade_does_not_flood_disks():
    _require_rdkit()
    doc = {
        "type": "mol",
        "smiles": "c1ccccc1O",
        "shade": {
            "atoms": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.95],
            "vmin": 0.0,
            "vmax": 1.0,
        },
    }
    DepictSpec.model_validate(doc)
    svg = render(doc, backend="rdkit")
    n_shade = len(re.findall(r'class="shade"', svg))
    assert n_shade > 0
    assert n_shade < 12, f"shade should stay readable, not flood disks ({n_shade})"


def test_committed_markush_svg_has_r1():
    path = EXAMPLES / "markush.svg"
    if not path.is_file():
        pytest.skip("gallery assets not generated yet")
    texts = _texts(path.read_text(encoding="utf-8"))
    assert "R1" in texts, f"{path.name} missing R1 — regenerate gallery SVGs"


def test_committed_star_r1_svg_has_subscript():
    path = EXAMPLES / "star_r1.svg"
    if not path.is_file():
        pytest.skip("gallery assets not generated yet")
    texts = _texts(path.read_text(encoding="utf-8"))
    assert "R₁" in texts, f"{path.name} missing R₁ — regenerate gallery SVGs"


def test_live_doc_rejects_rgroups_key():
    with pytest.raises(ValidationError):
        DepictSpec.model_validate(
            {"type": "mol", "smiles": "*C", "rgroups": ["$R_1$"]}
        )

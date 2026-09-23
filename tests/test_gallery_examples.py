"""Gallery example regressions — markup, nested docs, sparse shade."""

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


def test_gallery_markush_braced_cx_subscript():
    _require_rdkit()
    svg = render(
        {"type": "mol", "cxsmiles": "*c1ccccc1Cl |$R_{1};;;;;$|"},
        backend="rdkit",
    )
    texts = _texts(svg)
    assert "R₁" in texts, f"expected R₁ from R_{{1}} markup, got {texts}"


def test_gallery_phenol_star_cx_subscript():
    _require_rdkit()
    svg = render(
        {"type": "mol", "cxsmiles": "*c1ccc(O)cc1 |$R_{1};;;;;$|"},
        backend="rdkit",
    )
    texts = _texts(svg)
    assert "R₁" in texts, f"CX R_{{1}} must paint R₁, got {texts}"


def test_sparse_shade_paints_few_disks():
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
    assert n_shade < 12, f"sparse shade should not flood disks ({n_shade})"


def test_committed_markush_svg_has_subscript():
    path = EXAMPLES / "markush.svg"
    if not path.is_file():
        pytest.skip("gallery assets not generated yet")
    texts = _texts(path.read_text(encoding="utf-8"))
    assert "R₁" in texts, f"{path.name} missing R₁ — regenerate gallery SVGs"


def test_live_doc_rejects_rgroups_key():
    with pytest.raises(ValidationError):
        DepictSpec.model_validate(
            {"type": "mol", "smiles": "*C", "rgroups": ["$R_1$"]}
        )

"""Formal charge, stars/R-group names (native Rust depict)."""

from __future__ import annotations

import pytest

from xpict import Pict, render


def test_charge_label_in_svg():
    svg = render({"molecules": [{"smiles": "[NH4+]"}]})
    assert "N" in svg
    assert "+" in svg or "⁺" in svg or "＋" in svg
    assert "²⁺" not in svg


@pytest.mark.xfail(
    reason="native Chematic/CoordGen paint path does not yet emit radical dots",
    strict=True,
)
def test_radical_dot_in_svg():
    svg = render({"molecules": [{"smiles": "[CH3]"}]})
    assert "radical" in svg


def test_star_label():
    lay = Pict().layout({"molecules": [{"smiles": "*C"}]})[0]
    star = next(a for a in lay.atoms if a.element == "*")
    assert star.label == "*"
    svg = render({"molecules": [{"smiles": "*C"}]})
    assert "*" in svg


def test_star_name_from_cxsmiles():
    lay = Pict().layout({"molecules": [{"cxsmiles": "*C |$R1;$|"}]})[0]
    star = next(a for a in lay.atoms if a.element == "*")
    assert star.label == "R1"
    svg = render({"molecules": [{"cxsmiles": "*C |$R1;$|"}]})
    assert "R1" in svg


def test_render_align_grid():
    svg = render(
        {
            "molecules": [
                {"smiles": "c1ccc(cc1)O"},
                {"smiles": "COc1ccccc1"},
            ],
            "diagram": {"kind": "grid", "columns": 2},
        },
    )
    assert "<svg" in svg

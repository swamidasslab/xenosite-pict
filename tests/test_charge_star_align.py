"""Formal charge, radicals, stars/R-group names, and MCS alignment."""

from __future__ import annotations


import math
import pytest

from xpict import Pict, render
from xpict.align import _mcs_mapping, align_layouts



def test_charge_label_in_svg():
    svg = render({"molecules": [{"smiles": "[NH4+]"}]})
    assert "N" in svg
    assert "+" in svg or "⁺" in svg or "＋" in svg
    # Regression: label already carried ``+``; do not paint NH₄²⁺.
    assert "²⁺" not in svg


def test_display_text_does_not_double_append_charge():
    from xpict.contracts.layout import AtomLayout
    from xpict.draw.drawable import display_text

    assert display_text(AtomLayout(index=0, element="N", x=0, y=0, charge=1, label="NH4+")) == "NH4+"
    assert display_text(AtomLayout(index=0, element="N", x=0, y=0, charge=1, label="NH4")) == "NH4+"
    assert display_text(AtomLayout(index=0, element="O", x=0, y=0, charge=-1, label="OH−")) == "OH−"


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
    assert ">" in svg and "*" in svg


def test_star_name_from_cxsmiles():
    lay = Pict().layout({"molecules": [{"cxsmiles": "*C |$R1;$|"}]})[0]
    star = next(a for a in lay.atoms if a.element == "*")
    assert star.label == "R1"
    svg = render({"molecules": [{"cxsmiles": "*C |$R1;$|"}]})
    assert "R1" in svg


def test_align_mcs_maps_phenol_scaffold():
    pict = Pict()
    ph = pict.layout({"molecules": [{"smiles": "c1ccc(cc1)O"}]})[0]
    an = pict.layout({"molecules": [{"smiles": "COc1ccccc1"}]})[0]
    mapping = _mcs_mapping(ph, an)
    assert mapping is not None
    assert len(mapping) >= 6  # phenyl carbons at least


@pytest.mark.xfail(
    reason="host align.py Kabsch path deferred; product align is Rust depict",
    strict=True,
)
def test_align_layouts_transforms_coords():
    pict = Pict()
    a = pict.layout({"molecules": [{"smiles": "c1ccc(cc1)O"}]})[0]
    b = pict.layout({"molecules": [{"smiles": "COc1ccccc1"}]})[0]
    # Flip B so alignment must rotate/translate.
    flipped = b.model_copy(
        update={"atoms": [at.model_copy(update={"x": -at.x, "y": -at.y + 5.0}) for at in b.atoms]}
    )
    aligned = align_layouts([a, flipped], enabled=True)
    assert len(aligned) == 2

    # Ring centroids should be closer after alignment than raw flipped.
    def centroid(lay):
        xs = [at.x for at in lay.atoms]
        ys = [at.y for at in lay.atoms]
        return sum(xs) / len(xs), sum(ys) / len(ys)

    ca = centroid(aligned[0])
    cb = centroid(aligned[1])
    dist = math.hypot(ca[0] - cb[0], ca[1] - cb[1])
    assert dist < 2.0


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

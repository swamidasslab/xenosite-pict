"""Formal charge, stars/R-group names (native Rust depict)."""

from __future__ import annotations

from xpict import Pict, render


def test_charge_label_in_svg():
    svg = render({"molecules": [{"smiles": "[NH4+]"}]})
    assert "N" in svg
    # Implicit H on ammonium must appear (NH₄⁺ / H₄N⁺), not bare N⁺.
    assert "H" in svg or "₄" in svg
    assert "+" in svg or "⁺" in svg or "＋" in svg
    assert "²⁺" not in svg


def test_heteroatom_implicit_h_in_svg():
    ethanol = render({"molecules": [{"smiles": "CCO"}]})
    assert 'data-text="OH"' in ethanol or 'data-text="HO"' in ethanol
    aniline = render({"molecules": [{"smiles": "c1ccccc1N"}]})
    assert "NH" in aniline or "H₂N" in aniline or "NH₂" in aniline


def test_radical_dot_in_svg():
    svg = render({"molecules": [{"smiles": "[CH3]"}]})
    assert "radical" in svg
    assert "<circle" in svg


def test_ethyl_radical_depicts_ch2_and_dot():
    spec = {"molecules": [{"smiles": "C[CH2]"}]}
    lay = Pict().layout(spec)[0]
    rads = [a for a in lay.atoms if a.radical]
    assert len(rads) == 1
    assert rads[0].radical == 1
    assert rads[0].label in ("CH2", "H2C")
    svg = render(spec)
    assert "radical" in svg
    assert "<circle" in svg
    assert "CH₂" in svg or "CH2" in svg or "H₂C" in svg or "H2C" in svg


def test_hydroxy_radical_depicts_oh_and_dot():
    svg = render({"molecules": [{"smiles": "[OH]"}]})
    assert "radical" in svg
    assert "OH" in svg or "HO" in svg


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

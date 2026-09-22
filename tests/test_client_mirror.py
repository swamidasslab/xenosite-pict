"""Python client favors objects/methods; JS keeps a flat namespace."""

from __future__ import annotations

from xpict.client import Mol, render, to_svg, xpict


def test_client_benzene_svg_methods():
    r = Mol.from_source("c1ccccc1").render()
    svg = r.to_svg()
    assert "<svg" in svg
    assert r.width > 0 and r.height > 0


def test_client_cx_star_auto_label():
    r = xpict.mol("*C |$R1;$|").render()
    svg = r.to_svg()
    assert 'data-text="R1"' in svg


def test_client_multi_star_second_only():
    r = Mol.from_source("*C* |$;;R2;$|").render()
    by_idx = {a["index"]: a for a in r.molecule["atoms"]}
    assert by_idx[2]["label"] == "R2"


def test_client_align_via_method():
    phenol = Mol.from_source("c1ccccc1O").render()
    anisole = phenol.render_aligned("c1ccccc1OC")
    assert anisole.width > 0
    assert "<svg" in to_svg(anisole)


def test_module_helpers_still_work():
    """JS-shaped helpers remain for cross-language examples."""
    r = render(Mol.from_source("CCO"))
    assert "<svg" in to_svg(r.scene)

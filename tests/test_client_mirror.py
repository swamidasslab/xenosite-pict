"""JS-mirrored Python client uses Rust depict_molecule."""

from __future__ import annotations

from xpict.client import mol, render, to_svg, xpict


def test_client_benzene_svg():
    r = render(mol("c1ccccc1"))
    svg = to_svg(r.scene)
    assert "<svg" in svg
    assert r.width > 0 and r.height > 0


def test_client_cx_star_auto_label():
    r = xpict.render(xpict.mol("*C |$R1;$|"))
    svg = xpict.to_svg(r.scene)
    assert 'data-text="R1"' in svg


def test_client_multi_star_second_only():
    r = render(mol("*C* |$;;R2;$|"))
    stars = [a for a in r.molecule["atoms"] if a.get("z") == 0 or a.get("element") == "*"]
    assert len(stars) == 2
    assert stars[0].get("label", "*") in (None, "*") or stars[0].get("label") == "*"
    # second star labeled R2 by atom index
    by_idx = {a["index"]: a for a in r.molecule["atoms"]}
    assert by_idx[2]["label"] == "R2"

"""Atom-label orientation: center glyph on atom, traveling H flips OH→HO."""

from __future__ import annotations

import pytest

from xpict import render
from xpict.draw.label_place import compose_atom_label, place_backbone, split_atom_label
from xpict.draw.metrics import FONT_PX, LABEL_GAP_PX
from xpict.draw.text_metrics import measure_text


def test_split_oh_traveling():
    c, t = split_atom_label("OH")
    assert c == "O" and t == "H"
    c, t = split_atom_label("NH2")
    assert c == "N" and t == "H2"
    c, t = split_atom_label("Cl")
    assert c == "Cl" and t == ""
    c, t = split_atom_label("GlcA")
    assert c == "GlcA" and t == ""


def test_compose_flips_oh_on_west():
    assert compose_atom_label("O", "H", "east") == "OH"
    assert compose_atom_label("O", "H", "west") == "HO"
    assert compose_atom_label("GlcA", "", "west") == "GlcA"


def test_place_backbone_oh_on_left_is_ho():
    """Neighbor to the right → West orientation → HO with O on the atom."""
    coords = [(0.0, 0.0), (40.0, 0.0)]
    texts = ["OH", None]
    bonds = [(0, 1)]
    ends, labels = place_backbone(coords, texts, bonds)
    assert labels[0] is not None
    assert labels[0].text == "HO"
    assert labels[0].side == "west"
    o_adv = measure_text("O", FONT_PX).advance
    h_adv = measure_text("H", FONT_PX).advance
    assert labels[0].origin_x + h_adv + 0.5 * o_adv == pytest.approx(0.0, abs=1e-5)
    assert labels[0].clearance == pytest.approx(0.5 * o_adv + LABEL_GAP_PX)
    assert ends[0][0] == pytest.approx(labels[0].clearance)


def test_place_backbone_oh_on_right_is_oh():
    coords = [(0.0, 0.0), (40.0, 0.0)]
    texts = [None, "OH"]
    bonds = [(0, 1)]
    ends, labels = place_backbone(coords, texts, bonds)
    assert labels[1] is not None
    assert labels[1].text == "OH"
    assert labels[1].side == "east"
    o_adv = measure_text("O", FONT_PX).advance
    assert labels[1].origin_x + 0.5 * o_adv == pytest.approx(40.0, abs=1e-5)
    assert ends[0][2] == pytest.approx(40.0 - labels[1].clearance)


def test_glca_centers_first_or_last_glyph():
    coords = [(0.0, 0.0), (60.0, 0.0)]
    texts = ["GlcA", None]
    bonds = [(0, 1)]
    _, labels = place_backbone(coords, texts, bonds)
    assert labels[0] is not None
    assert labels[0].text == "GlcA"
    a_adv = measure_text("A", FONT_PX).advance
    prefix = measure_text("Glc", FONT_PX).advance
    assert labels[0].origin_x + prefix + 0.5 * a_adv == pytest.approx(0.0, abs=1e-4)


def test_phenol_svg_uses_oriented_label():
    svg = render({"molecules": [{"smiles": "c1ccccc1O"}]}, backend="native")
    # Native layout puts the hydroxyl on the left → West → HO.
    assert 'data-text="HO"' in svg


def test_ethanol_oh_is_east_oriented():
    svg = render({"molecules": [{"smiles": "CCO"}]}, backend="native")
    assert 'data-text="OH"' in svg

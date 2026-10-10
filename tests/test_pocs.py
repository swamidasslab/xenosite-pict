"""POC-focused tests: marks/shade drawing and reaction_scheme compose."""

from __future__ import annotations

import pytest

from xpict import Pict, depict, render



def test_own_svg_has_bonds_and_labels():
    svg = render({"molecules": [{"smiles": "CCO"}]})
    assert "<svg" in svg
    assert 'class="bond' in svg or "bond-" in svg
    assert "O" in svg  # heteroatom label


def test_shade_and_marks_layers():
    pict = Pict()
    n = len(pict.layout({"molecules": [{"smiles": "CCO"}]})[0].atoms)
    svg = pict.render(
        {
            "molecules": [
                {
                    "smiles": "CCO",
                    "shade": {"atoms": [0.2 * i for i in range(n)]},
                    "marks": [
                        {"kind": "atoms", "atoms": [n - 1], "color": "#c00"},
                        {"kind": "substructure", "atoms": list(range(n)), "color": "#06c"},
                    ],
                }
            ]
        }
    )
    assert "shade" in svg or "plot-dot" in svg or "<circle" in svg


def test_network_scheme_places_distinct_viewports():
    rows = depict(
        {
            "type": "reaction_scheme",
            "children": [
                {"type": "mol", "id": "A", "smiles": "CCO"},
                {"type": "mol", "id": "B", "smiles": "C"},
                {"type": "mol", "id": "C", "smiles": "O"},
                {"type": "edge", "sources": "A", "targets": "B"},
                {"type": "edge", "sources": "B", "targets": "C"},
            ],
        }
    )
    assert len(rows) == 1
    vps = rows[0].scene.viewports
    assert len(vps) == 3
    positions = {(round(vp.x, 1), round(vp.y, 1)) for vp in vps}
    assert len(positions) >= 2

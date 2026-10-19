"""Molecule ink color (backbone bonds + atom labels)."""

from __future__ import annotations


import re

from xpict import render


def test_mol_color_tints_bonds_and_labels():
    svg = render(
        {"molecules": [{"smiles": "CCO", "color": "#06c"}]},
    )
    # Bond strokes use the override.
    assert re.search(r'class="[^"]*bond-skeleton[^"]*"[^>]*stroke="#06c"', svg) or re.search(
        r'stroke="#06c"[^>]*class="[^"]*bond-skeleton', svg
    )
    # Atom label glyph path uses the same fill (OH / HO from implicit H).
    assert re.search(
        r'data-text="(?:OH|HO)"[^>]*fill="#06c"|fill="#06c"[^>]*data-text="(?:OH|HO)"',
        svg,
    )


def test_default_ink_is_near_black():
    svg = render({"molecules": [{"smiles": "CCO"}]})
    assert 'stroke="#111"' in svg
    assert re.search(
        r'data-text="(?:OH|HO)"[^>]*fill="#111"|fill="#111"[^>]*data-text="(?:OH|HO)"',
        svg,
    )

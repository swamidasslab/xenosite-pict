"""Native organic SMILES layout — rings + 120° chains."""

from __future__ import annotations

import math

import pytest

from xenosite.pict import Pict, render
from xenosite.pict.backends.native_smiles import parse_organic_smiles
from xenosite.pict.draw.rings import (
    all_rings_can_be_regular_polygons,
    find_sssr,
)


def _native_layout(smiles: str):
    return Pict(backend="native").layout({"molecules": [{"smiles": smiles}]}).molecules[0]


def test_parse_benzene_has_six_ring_bonds():
    mol = parse_organic_smiles("c1ccccc1")
    assert len(mol.atoms) == 6
    assert len(mol.bonds) == 6


def test_hexane_is_120_zigzag():
    lay = _native_layout("CCCCCC")
    pos = {a.index: (a.x, a.y) for a in lay.atoms}
    angles = []
    for i in range(1, 5):
        ax, ay = pos[i - 1]
        bx, by = pos[i]
        cx, cy = pos[i + 1]
        v1 = (ax - bx, ay - by)
        v2 = (cx - bx, cy - by)
        n1 = math.hypot(*v1)
        n2 = math.hypot(*v2)
        cos = max(-1.0, min(1.0, (v1[0] * v2[0] + v1[1] * v2[1]) / (n1 * n2)))
        angles.append(math.degrees(math.acos(cos)))
    assert all(abs(a - 120.0) < 1e-6 for a in angles)


def test_benzene_is_regular_hexagon():
    lay = _native_layout("c1ccccc1")
    rings = find_sssr(lay)
    assert len(rings) == 1
    assert rings[0].size == 6
    pos = {a.index: (a.x, a.y) for a in lay.atoms}
    bond_lens = []
    for b in lay.bonds:
        x1, y1 = pos[b.begin]
        x2, y2 = pos[b.end]
        bond_lens.append(math.hypot(x2 - x1, y2 - y1))
    assert max(bond_lens) - min(bond_lens) < 1e-9


def test_naphthalene_two_fused_regular_faces():
    lay = _native_layout("c1ccc2ccccc2c1")
    rings = find_sssr(lay)
    assert len(rings) == 2
    assert all_rings_can_be_regular_polygons(rings)
    assert len(lay.atoms) == 10
    assert len(lay.bonds) == 11


def test_norbornane_does_not_claim_all_regular():
    lay = _native_layout("C1CC2CCC1C2")
    rings = find_sssr(lay)
    assert len(rings) >= 2
    assert all_rings_can_be_regular_polygons(rings) is False
    assert any("bridged" in w.lower() or "cage" in w.lower() for w in lay.warnings)


def test_native_svg_renders():
    svg = render({"molecules": [{"smiles": "c1ccccc1O"}]}, backend="native")
    assert "<svg" in svg
    assert "bond" in svg

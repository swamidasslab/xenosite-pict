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


def test_acetic_acid_carbonyl_is_trigonal():
    lay = _native_layout("CC(=O)O")
    # Carbonyl carbon is the degree-3 atom.
    from collections import Counter

    deg = Counter()
    for b in lay.bonds:
        deg[b.begin] += 1
        deg[b.end] += 1
    center = max(deg, key=deg.get)
    pos = {a.index: (a.x, a.y) for a in lay.atoms}
    nbrs = [
        b.end if b.begin == center else b.begin
        for b in lay.bonds
        if b.begin == center or b.end == center
    ]
    cx, cy = pos[center]
    vecs = [(pos[n][0] - cx, pos[n][1] - cy) for n in nbrs]
    angles = []
    for i in range(len(vecs)):
        for j in range(i + 1, len(vecs)):
            v1, v2 = vecs[i], vecs[j]
            n1, n2 = math.hypot(*v1), math.hypot(*v2)
            cos = max(-1.0, min(1.0, (v1[0] * v2[0] + v1[1] * v2[1]) / (n1 * n2)))
            angles.append(math.degrees(math.acos(cos)))
    assert all(abs(a - 120.0) < 1e-6 for a in angles)


def test_phenol_substituent_is_exterior():
    lay = _native_layout("c1ccc(cc1)O")
    assert len(lay.atoms) == 7
    rings = find_sssr(lay)
    assert len(rings) == 1
    # Oxygen is the heteroatom outside the ring.
    o = next(a for a in lay.atoms if a.element == "O")
    ring_atoms = set(rings[0].atoms)
    assert o.index not in ring_atoms


def test_anthracene_fused_bonds_stay_regular():
    """Flip must mirror across the shared edge, not world X (linear acene)."""
    lay = _native_layout("c1ccc2c(c1)ccc3ccccc32")
    rings = find_sssr(lay)
    assert len(rings) == 3
    assert all_rings_can_be_regular_polygons(rings)
    pos = {a.index: (a.x, a.y) for a in lay.atoms}
    bond_lens = [
        math.hypot(pos[b.end][0] - pos[b.begin][0], pos[b.end][1] - pos[b.begin][1])
        for b in lay.bonds
    ]
    assert max(bond_lens) - min(bond_lens) < 1e-6


def test_aspirin_parses_and_places_all_atoms():
    lay = _native_layout("CC(=O)Oc1ccccc1C(=O)O")
    assert len(lay.atoms) == 13
    assert len(lay.bonds) == 13
    pos = {a.index: (a.x, a.y) for a in lay.atoms}
    # Every bond roughly unit length for the native bond constant.
    for b in lay.bonds:
        d = math.hypot(pos[b.end][0] - pos[b.begin][0], pos[b.end][1] - pos[b.begin][1])
        assert abs(d - 1.5) < 1e-6


def test_heteroatom_labels_include_implicit_h():
    lay = _native_layout("c1ccc[nH]1")
    labels = {a.label for a in lay.atoms if a.label}
    assert "NH" in labels
    lay2 = _native_layout("CCO")
    assert any(a.label == "OH" for a in lay2.atoms)


def test_native_chiral_smiles_gets_wedge():
    lay = _native_layout("C[C@H](O)Cl")
    stereos = [b for b in lay.bonds if b.stereo in {"up", "down"}]
    assert stereos, "expected at least one wedge from @ SMILES"
    # Thin end at stereocenter (carbon index 1).
    assert all(b.begin == 1 for b in stereos)
    svg = render({"molecules": [{"smiles": "C[C@H](O)Cl"}]}, backend="native")
    assert "bond-wedge" in svg

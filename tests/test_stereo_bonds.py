"""Stereo + skeleton-first bond drawing tests (RDKit/Indigo conventions)."""

from __future__ import annotations

import pytest

from xenosite.pict import Pict, render
from xenosite.pict.draw.bonds import bond_strokes, hashed_wedge, solid_wedge
from xenosite.pict.perception import chematic_available, perceive_smiles


def _backend() -> str:
    try:
        Pict(backend="indigo").layout({"molecules": [{"smiles": "CCO"}]})
        return "indigo"
    except Exception:
        pytest.skip("indigo not installed")


def test_skeleton_then_offset_for_double():
    strokes = bond_strokes(0, 0, 20, 0, 2.0)
    assert strokes.skeleton is not None
    assert "bond-skeleton" in (strokes.skeleton.cls or "")
    assert len(strokes.offsets) == 1
    assert "bond-offset" in (strokes.offsets[0].cls or "")
    # Acyclic offset runs the full skeleton, not the ring inset.
    assert "0.00" in strokes.offsets[0].d and "20.00" in strokes.offsets[0].d


def test_triple_has_skeleton_and_two_offsets():
    strokes = bond_strokes(0, 0, 30, 0, 3.0)
    assert strokes.skeleton is not None
    assert len(strokes.offsets) == 2


def test_solid_wedge_tip_at_begin():
    w = solid_wedge(1.0, 2.0, 11.0, 2.0)
    assert w.d.startswith("M 1.00 2.00")
    assert w.fill == "#111"
    assert "wedge-up" in (w.cls or "")


def test_hashed_wedge_has_multiple_dashes():
    dashes = hashed_wedge(0, 0, 20, 0)
    assert len(dashes) >= 4


def test_stereo_up_replaces_skeleton():
    strokes = bond_strokes(0, 0, 10, 0, 1.0, stereo="up")
    assert strokes.skeleton is None
    assert strokes.stereo and "wedge-up" in (strokes.stereo[0].cls or "")


def test_indigo_layout_sets_wedge_stereo():
    backend = _backend()
    layout = (
        Pict(backend=backend)
        .layout({"molecules": [{"smiles": "C[C@H](O)Cl"}]})
        .molecules[0]
    )
    stereos = {b.stereo for b in layout.bonds if b.stereo and b.stereo != "none"}
    assert stereos & {"up", "down"}, f"expected wedge stereo, got {[b.stereo for b in layout.bonds]}"


def test_chiral_molecule_svg_contains_wedge_or_hash():
    backend = _backend()
    svg = render({"molecules": [{"smiles": "C[C@H](O)Cl"}]}, backend=backend)
    assert "<svg" in svg
    assert "bond-wedge" in svg, "expected solid or hashed wedge class in SVG"
    assert "bond-skeleton" in svg


def test_either_single_is_wavy():
    strokes = bond_strokes(0, 0, 20, 0, 1.0, stereo="either")
    assert strokes.skeleton is None
    assert strokes.stereo and "bond-either" in (strokes.stereo[0].cls or "")


def test_either_double_is_crossed():
    strokes = bond_strokes(0, 0, 20, 0, 2.0, stereo="either")
    assert any("either-cross" in (p.cls or "") for p in strokes.stereo)


def test_chematic_perception_optional():
    if not chematic_available():
        pytest.skip("chematic not installed")
    mol = perceive_smiles("c1ccccc1")
    assert mol.engine == "chematic"
    assert len(mol.atoms) == 6
    assert any(b.aromatic or b.order == 1.5 for b in mol.bonds) or len(mol.bonds) == 6
    assert any("perception only" in w for w in mol.warnings)

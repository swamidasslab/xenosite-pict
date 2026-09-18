"""Alignment interface: RDKit template depiction, rigid fallback."""

from __future__ import annotations

import math

import pytest

from xenosite.pict import Pict, render
from xenosite.pict.align import RigidAligner, align_layouts, align_to_reference, select_aligner
from xenosite.pict.align_rdkit import RdkitAligner, rdkit_available


def _backend() -> str:
    try:
        Pict(backend="indigo").layout({"molecules": [{"smiles": "C"}]})
        return "indigo"
    except Exception:
        return "native"


def _layout(smiles: str):
    return Pict(backend=_backend()).layout({"molecules": [{"smiles": smiles}]}).molecules[0]


def _rmsd(ref, other, mapping: dict[int, int]) -> float:
    ref_at = {a.index: a for a in ref.atoms}
    oth_at = {a.index: a for a in other.atoms}
    err = [
        math.hypot(oth_at[o].x - ref_at[r].x, oth_at[o].y - ref_at[r].y)
        for o, r in mapping.items()
        if o in oth_at and r in ref_at
    ]
    assert err
    return sum(err) / len(err)


def _stretch(layout, sx: float):
    return layout.model_copy(
        update={"atoms": [a.model_copy(update={"x": a.x * sx}) for a in layout.atoms]}
    )


def test_rigid_aligner_cannot_template():
    aligner = RigidAligner()
    assert aligner.supports_template is False
    ref = _layout("c1ccccc1")
    other = _layout("c1ccccc1Cl")
    mapping = aligner.map_atoms(ref, other)
    assert mapping is not None
    assert aligner.depict_on_template(ref, other, mapping) is None


def test_rigid_align_moves_flipped_molecule():
    ref = _layout("c1ccc(cc1)O")
    other = _layout("COc1ccccc1")
    flipped = other.model_copy(
        update={
            "atoms": [a.model_copy(update={"x": -a.x, "y": -a.y + 5.0}) for a in other.atoms]
        }
    )
    aligned = align_layouts([ref, flipped], enabled=True, aligner=RigidAligner())
    assert "alignment: rigid transform" in aligned[1].warnings
    mapping = RigidAligner().map_atoms(ref, aligned[1])
    assert mapping is not None
    raw = _rmsd(ref, flipped, mapping)
    snapped = _rmsd(ref, aligned[1], mapping)
    assert snapped < 1e-6


def test_rigid_align_lands_the_oxygen():
    """The ring+O of anisole is the same shape as phenol, so rotation puts O on O."""
    ref = _layout("c1ccc(cc1)O")
    other = _layout("COc1ccccc1")
    aligner = RigidAligner()
    mapping = aligner.map_atoms(ref, other)
    assert mapping is not None
    aligned = aligner.rigid_align(ref, other, mapping)
    assert _rmsd(ref, aligned, mapping) < 1e-6


def test_select_aligner_prefers_rdkit_when_installed():
    aligner = select_aligner()
    if rdkit_available():
        assert aligner.name == "rdkit"
        assert aligner.supports_template is True
    else:
        assert aligner.name == "rigid"
        assert aligner.supports_template is False


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_template_locks_distorted_core():
    """A rigid move cannot put a regular ring onto a stretched one. Template can."""
    ref = _stretch(_layout("c1ccccc1"), 1.8)
    other = _layout("c1ccccc1Cl")
    aligner = RdkitAligner()
    mapping = aligner.map_atoms(ref, other)
    assert mapping is not None and len(mapping) >= 6
    templated = aligner.depict_on_template(ref, other, mapping, smiles="c1ccccc1Cl")
    assert templated is not None
    assert _rmsd(ref, templated, mapping) < 1e-6
    assert "alignment: rdkit template depiction" in templated.warnings
    rigid = RigidAligner().rigid_align(ref, other, mapping)
    assert _rmsd(ref, rigid, mapping) > 0.05


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_template_new_bond_matches_reference_length():
    ref = _layout("c1ccc(cc1)O")
    other = _layout("COc1ccccc1")
    aligner = RdkitAligner()
    mapping = aligner.map_atoms(ref, other)
    assert mapping is not None
    templated = align_to_reference(ref, other, aligner, smiles="COc1ccccc1")
    assert templated is not None
    by_index = {a.index: a for a in templated.atoms}
    ref_at = {a.index: a for a in ref.atoms}
    ref_len = sum(
        math.hypot(ref_at[b.begin].x - ref_at[b.end].x, ref_at[b.begin].y - ref_at[b.end].y)
        for b in ref.bonds
    ) / len(ref.bonds)
    extra = []
    mapped = set(mapping)
    for bond in templated.bonds:
        outside = (bond.begin in mapped) ^ (bond.end in mapped)
        if not outside:
            continue
        a, b = by_index[bond.begin], by_index[bond.end]
        extra.append(math.hypot(a.x - b.x, a.y - b.y))
    assert extra, "expected a bond from the core to the methoxy carbon"
    assert extra[0] == pytest.approx(ref_len, rel=0.15)


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_template_failure_uses_rigid_fallback(monkeypatch):
    import rdkit.Chem.rdDepictor as depictor

    def _boom(*_args, **_kwargs):
        raise RuntimeError("depict failed")

    monkeypatch.setattr(depictor, "Compute2DCoords", _boom)
    ref = _layout("c1ccccc1O")
    other = _layout("COc1ccccc1")
    flipped = other.model_copy(
        update={"atoms": [a.model_copy(update={"x": -a.x + 3, "y": -a.y}) for a in other.atoms]}
    )
    aligned = align_to_reference(ref, flipped, RdkitAligner(), smiles="COc1ccccc1")
    assert any("rigid fallback" in w for w in aligned.warnings)
    mapping = RdkitAligner().map_atoms(ref, other)
    assert mapping is not None
    assert _rmsd(ref, aligned, mapping) < _rmsd(ref, flipped, mapping)


def test_render_aligned_pair():
    svg = render(
        {
            "molecules": [
                {"smiles": "c1ccc(cc1)O"},
                {"smiles": "COc1ccccc1"},
            ],
            "diagram": {"kind": "grid", "columns": 2, "align": True},
        },
        backend=_backend(),
    )
    assert "<svg" in svg
    assert svg.count("<svg") == 1 or svg.count("pict-mol") >= 1

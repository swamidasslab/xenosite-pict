"""Alignment interface: RDKit template depiction, rigid fallback."""

from __future__ import annotations

import math

import pytest

from xpict import Pict, render
from xpict.align import RigidAligner, align_layouts, align_to_reference, select_aligner
from xpict.align_rdkit import RdkitAligner, rdkit_available


def _backend() -> str:
    """MVP layout backend (indigo is out of scope for now)."""
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
        update={"atoms": [a.model_copy(update={"x": -a.x, "y": -a.y + 5.0}) for a in other.atoms]}
    )
    aligned = align_layouts([ref, flipped], enabled=True, aligner=RigidAligner())
    assert "alignment: rigid transform" in aligned[1].warnings
    mapping = RigidAligner().map_atoms(ref, aligned[1])
    assert mapping is not None
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


def test_reflection_keeps_smiles_chirality():
    """A mirror inverts the drawing, so up and down swap and the SMILES tag still holds."""
    pytest.importorskip("rdkit")
    from rdkit import Chem
    from rdkit.Geometry import Point3D

    smiles = "C[C@H](O)Cl"
    original = Pict(backend="native").layout({"molecules": [{"smiles": smiles}]}).molecules[0]
    mirror = original.model_copy(
        update={"atoms": [a.model_copy(update={"y": -a.y}) for a in original.atoms]}
    )
    mapping = {a.index: a.index for a in original.atoms}
    aligned = RigidAligner().rigid_align(mirror, original, mapping)
    assert [(b.begin, b.end, b.stereo) for b in aligned.bonds] != [
        (b.begin, b.end, b.stereo) for b in original.bonds
    ]

    em = Chem.EditableMol(Chem.Mol())
    idx = {}
    for atom in sorted(aligned.atoms, key=lambda a: a.index):
        idx[atom.index] = em.AddAtom(Chem.Atom(atom.element))
    for bond in aligned.bonds:
        kind = Chem.BondType.DOUBLE if bond.order >= 1.5 else Chem.BondType.SINGLE
        em.AddBond(idx[bond.begin], idx[bond.end], kind)
    mol = em.GetMol()
    Chem.SanitizeMol(mol)
    conf = Chem.Conformer(mol.GetNumAtoms())
    for atom in aligned.atoms:
        conf.SetAtomPosition(idx[atom.index], Point3D(atom.x, atom.y, 0.0))
    mol.AddConformer(conf)
    for bond in aligned.bonds:
        if bond.stereo not in {"up", "down"}:
            continue
        rb = mol.GetBondBetweenAtoms(idx[bond.begin], idx[bond.end])
        assert rb.GetBeginAtomIdx() == idx[bond.begin]
        rb.SetBondDir(Chem.BondDir.BEGINWEDGE if bond.stereo == "up" else Chem.BondDir.BEGINDASH)
    Chem.AssignChiralTypesFromBondDirs(mol)
    got = next(
        a.GetChiralTag()
        for a in mol.GetAtoms()
        if a.GetChiralTag() != Chem.ChiralType.CHI_UNSPECIFIED
    )
    want = Chem.MolFromSmiles(smiles).GetAtomWithIdx(1).GetChiralTag()
    assert got == want


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


def test_outlier_does_not_dislodge_the_core():
    """One atom off the reference is left out. The rest stay put.

    Fitting that atom too would slide the whole scaffold.
    """
    ref = _layout("CCc1ccccc1")
    other = _layout("CCc1ccccc1")
    degree: dict[int, int] = {}
    for bond in other.bonds:
        degree[bond.begin] = degree.get(bond.begin, 0) + 1
        degree[bond.end] = degree.get(bond.end, 0) + 1
    leaf = next(
        atom.index for atom in other.atoms if atom.element == "C" and degree.get(atom.index, 0) == 1
    )
    moved = other.model_copy(
        update={
            "atoms": [
                atom.model_copy(update={"x": atom.x + 4.0, "y": atom.y - 3.0})
                if atom.index == leaf
                else atom
                for atom in other.atoms
            ]
        }
    )
    aligner = RigidAligner()
    whole = aligner.map_atoms(ref, other)
    assert whole is not None and leaf in whole
    kept = aligner.map_atoms(ref, moved)
    assert kept is not None
    assert leaf not in kept
    assert len(kept) >= len(whole) - 1
    assert _rmsd(ref, aligner.rigid_align(ref, moved, kept), kept) < 1e-3
    # The same atoms plus the outlier do not sit on the reference.
    dislodged = _rmsd(ref, aligner.rigid_align(ref, moved, whole), whole)
    assert dislodged > 0.05


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
    assert svg.count("<svg") == 1 or svg.count("xpict-mol") >= 1


def test_correspondence_is_not_the_first_embedding():
    """A symmetric core has many embeddings. The first is a local choice.

    These pairs are the same shape on the common subgraph, so the right
    embedding has rigid RMSD ~ 0. The first substructure hit does not.
    """
    pairs = [
        ("c1ccc(cc1)C(=O)O", "c1ccc(cc1)C(N)=O"),
        ("COc1ccc(C)cc1", "Oc1ccc(OC)cc1"),
        ("CC(C)Cc1ccc(cc1)C(C)C(=O)O", "CC(C)Cc1ccc(cc1)C(C)C"),
    ]
    aligners = [RigidAligner()]
    if rdkit_available():
        aligners.append(RdkitAligner())
    for ref_smi, other_smi in pairs:
        ref = _layout(ref_smi)
        other = _layout(other_smi)
        for aligner in aligners:
            mapping = aligner.map_atoms(ref, other)
            assert mapping is not None, (ref_smi, aligner.name)
            aligned = aligner.rigid_align(ref, other, mapping)
            err = _rmsd(ref, aligned, mapping)
            assert err < 1e-3, (ref_smi, aligner.name, len(mapping), err)
            if aligner.supports_template:
                templated = aligner.depict_on_template(ref, other, mapping, smiles=other_smi)
                assert templated is not None
                assert _rmsd(ref, templated, mapping) < 1e-4

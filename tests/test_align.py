"""Alignment interface: RDKit template depiction, rigid fallback."""

from __future__ import annotations

from helpers import layout_backend

import math

import pytest

from xpict import Pict, render
from xpict.align import RigidAligner, align_layouts, align_to_reference, select_aligner
from xpict.align_rdkit import RdkitAligner, rdkit_available


def _backend() -> str:
    return layout_backend()


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
    original = Pict(backend=layout_backend()).layout({"molecules": [{"smiles": smiles}]}).molecules[0]
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
    # Depictor MCS match may differ from map_atoms embedding — check overlay.
    hits = sum(
        1
        for a in templated.atoms
        if any(math.hypot(a.x - b.x, a.y - b.y) < 0.2 for b in ref.atoms)
    )
    assert hits >= 6, hits
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

    monkeypatch.setattr(depictor, "GenerateDepictionMatching2DStructure", _boom)
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
    """Symmetric cores have many embeddings; choose_mapping picks a rigid fit.

    Assert rigid RMSD on the chosen map. Template path: overlay hits only
    (Depictor may pick a symmetrically equivalent embedding).
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
                hits = sum(
                    1
                    for a in templated.atoms
                    if any(math.hypot(a.x - b.x, a.y - b.y) < 0.2 for b in ref.atoms)
                )
                assert hits >= 6, (ref_smi, aligner.name, hits)


def _overlay_hits(ref, other, tol: float = 0.2) -> int:
    return sum(
        1
        for a in other.atoms
        if any(math.hypot(a.x - b.x, a.y - b.y) < tol for b in ref.atoms)
    )


def _coords_key(layout) -> tuple:
    return tuple((round(a.x, 6), round(a.y, 6)) for a in sorted(layout.atoms, key=lambda x: x.index))


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_asymmetric_para_halo():
    """Para-F/Cl vs para-F/Br — F coincides (breaks ring symmetry)."""
    ref = _layout("Fc1ccc(Cl)cc1")
    other = _layout("Fc1ccc(Br)cc1")
    aligner = RdkitAligner()
    aligned = align_to_reference(ref, other, aligner, smiles="Fc1ccc(Br)cc1")
    assert _overlay_hits(ref, aligned) >= 7
    f_ref = next(a for a in ref.atoms if a.element == "F")
    f_q = next(a for a in aligned.atoms if a.element == "F")
    assert math.hypot(f_ref.x - f_q.x, f_ref.y - f_q.y) < 0.2


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_mcs_phenol_quinone_both_ways():
    """Element+hybridization MCS: aromatic phenol ↔ benzoquinone (O anchors)."""
    phenol = _layout("c1ccc(O)cc1")
    quinone = _layout("O=C1C=CC(=O)C=C1")
    aligner = RdkitAligner()
    q_on_ph = align_to_reference(phenol, quinone, aligner, smiles="O=C1C=CC(=O)C=C1")
    assert _overlay_hits(phenol, q_on_ph) >= 6
    ph_on_q = align_to_reference(quinone, phenol, aligner, smiles="c1ccc(O)cc1")
    assert _overlay_hits(quinone, ph_on_q) >= 6


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_mcs_aniline_quinone_imine():
    """Amino N matches quinone-imine N (both N/SP2); not benzoquinone O."""
    aniline = _layout("Nc1ccccc1")
    imine = _layout("O=C1C=CC(=N)C=C1")
    quinone = _layout("O=C1C=CC(=O)C=C1")
    aligner = RdkitAligner()
    im_on_an = align_to_reference(aniline, imine, aligner, smiles="O=C1C=CC(=N)C=C1")
    assert _overlay_hits(aniline, im_on_an) >= 6
    from xpict.align_rdkit import _fmcs_mapping

    # Ring-only MCS vs benzoquinone is OK (≥3); N does not map to O.
    m = _fmcs_mapping(aniline, quinone)
    assert m is not None and len(m) == 6


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_atom_map_align_thp_chain():
    """C1CCCOC1 ← O=CCCCCO: MCS finds atoms; Depictor gets atom map only."""
    from xpict.align_rdkit import _fmcs_mapping

    ring = _layout("C1CCCOC1")
    chain = _layout("O=CCCCCO")
    mapping = _fmcs_mapping(ring, chain)
    assert mapping is not None and len(mapping) >= 3
    aligned = align_to_reference(ring, chain, RdkitAligner(), smiles="O=CCCCCO")
    assert _overlay_hits(ring, aligned) >= 4


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_mcs_rejects_aliphatic_vs_quinone():
    """Cyclohexane ether must not MCS-align onto benzoquinone (SP3 ≠ SP2)."""
    from xpict.align_rdkit import _fmcs_mapping

    quinone = _layout("O=C1C=CC(=O)C=C1")
    chain = _layout("C1CCCCC1CCOCCCCCC")
    assert _fmcs_mapping(quinone, chain) is None
    assert _fmcs_mapping(chain, quinone) is None

    aligner = RdkitAligner()
    free = _layout("C1CCCCC1CCOCCCCCC")
    aligned = align_to_reference(quinone, chain, aligner, smiles="C1CCCCC1CCOCCCCCC")
    # No MCS → align_to_reference returns other unchanged.
    assert _coords_key(aligned) == _coords_key(free)
    assert _overlay_hits(quinone, aligned) < 4


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_mcs_ethyl_pentyl_both_ways():
    """Alkyl chain anchors; ring may flip — assert overlay hits, not index maps."""
    ethyl = _layout("c1ccccc1CC")
    pentyl = _layout("c1ccccc1CCCCC")
    aligner = RdkitAligner()
    p_on_e = align_to_reference(ethyl, pentyl, aligner, smiles="c1ccccc1CCCCC")
    assert _overlay_hits(ethyl, p_on_e) >= 8
    e_on_p = align_to_reference(pentyl, ethyl, aligner, smiles="c1ccccc1CC")
    assert _overlay_hits(pentyl, e_on_p) >= 8


@pytest.mark.skipif(not rdkit_available(), reason="rdkit not installed")
def test_rdkit_multi_query_leaves_template_coords():
    """Several queries can align onto one template without moving it."""
    ref = _layout("c1ccc(O)cc1")
    before = _coords_key(ref)
    aligner = RdkitAligner()
    queries = [
        "O=C1C=CC(=O)C=C1",
        "c1ccccc1CC",
        "c1ccccc1O",
    ]
    aligned = []
    for smi in queries:
        other = _layout(smi)
        out = align_to_reference(ref, other, aligner, smiles=smi)
        aligned.append(out)
        assert _coords_key(ref) == before, smi
        assert _overlay_hits(ref, out) >= 6, smi
    # Batch path (align_layouts) also keeps layouts[0] identity / coords.
    batch = align_layouts(
        [ref, *[ _layout(s) for s in queries ]],
        enabled=True,
        aligner=aligner,
        specs=None,
    )
    assert _coords_key(batch[0]) == before
    assert batch[0] is ref or _coords_key(batch[0]) == before

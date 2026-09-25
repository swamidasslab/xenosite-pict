"""Wedge stereo correctness vs RDKit (order-robust; no wedge-identity check).

Oracle: rebuild an RDKit molecule from xpict layout coordinates + ``up``/``down``
bond dirs, run ``AssignChiralTypesFromBondDirs``, and compare **canonical
isomeric SMILES** to the input SMILES ground truth.

What this suite does **not** require:

- That xpict wedges the same bonds RDKit's ``WedgeMolBonds`` would choose
- That ``up``/``down`` on a given bond match RDKit's sign on that bond
  (equivalent encodings may wedge a different substituent)

Wedges need only encode the correct absolute stereo.
"""

from __future__ import annotations

import pytest
from helpers import (
    isomeric_smiles,
    isomeric_smiles_from_layout,
    layout_backend,
    layout_to_rdkit_with_wedges,
    random_isomeric_smiles,
)

from xpict import Pict
from xpict.contracts.layout import BondLayout, MoleculeLayout

# Seeds spanning acyclic, aromatic, amino-acid, and ring stereo.
_CHIRAL_SEEDS: tuple[str, ...] = (
    "C[C@H](O)Cl",
    "C[C@@H](O)Cl",
    "CC[C@H](C)O",
    "C[C@H](N)C(=O)O",
    "N[C@@H](Cc1ccccc1)C(=O)O",
    "C[C@H](c1ccccc1)O",
    "C[C@H](O)[C@H](C)O",
    "C[C@H](O)[C@@H](C)O",
    "C[C@@H](O)[C@@H](C)O",
    "C[C@H]1CC[C@H](O)CC1",
    "C[C@H]1CC[C@@H](O)CC1",
    "C[C@](F)(Cl)Br",
    "C[C@@](F)(Cl)Br",
    "O[C@H]1CCCC1",
    "Cl[C@@H]1CCOCC1",
)


def _layout(smiles: str) -> MoleculeLayout:
    return Pict(backend=layout_backend()).layout({"molecules": [{"smiles": smiles}]})[0]


def _assert_stereo_correct(smiles: str, layout: MoleculeLayout | None = None) -> None:
    lay = layout if layout is not None else _layout(smiles)
    got = isomeric_smiles_from_layout(lay)
    want = isomeric_smiles(smiles)
    assert got == want, (
        f"stereo mismatch for {smiles!r}: layout wedges imply {got!r}, want {want!r}; "
        f"wedges={[ (b.begin, b.end, b.stereo) for b in lay.bonds if b.stereo in {'up', 'down'} ]}"
    )


@pytest.mark.parametrize("smiles", _CHIRAL_SEEDS)
def test_layout_wedges_match_smiles_stereo(smiles: str) -> None:
    """Each seed's wedges recover that seed's absolute stereo (RDKit oracle)."""
    _assert_stereo_correct(smiles)


@pytest.mark.parametrize("seed", _CHIRAL_SEEDS)
def test_stereo_robust_to_smiles_atom_order(seed: str) -> None:
    """Random SMILES rewrites of the same stereo still layout stereo-correctly."""
    want = isomeric_smiles(seed)
    variants = random_isomeric_smiles(seed, n=16)
    assert len(variants) >= 2, f"expected multiple writings for {seed!r}, got {variants}"
    for smiles in variants:
        assert isomeric_smiles(smiles) == want
        _assert_stereo_correct(smiles)


def test_enantiomer_pair_round_trips_to_distinct_smiles() -> None:
    """Opposite tetrahedral tags must not collapse to the same isomeric SMILES."""
    left, right = "C[C@H](O)Cl", "C[C@@H](O)Cl"
    assert isomeric_smiles(left) != isomeric_smiles(right)
    assert isomeric_smiles_from_layout(_layout(left)) == isomeric_smiles(left)
    assert isomeric_smiles_from_layout(_layout(right)) == isomeric_smiles(right)
    assert isomeric_smiles_from_layout(_layout(left)) != isomeric_smiles_from_layout(
        _layout(right)
    )


def test_inverting_all_wedge_signs_flips_enantiomer() -> None:
    """Oracle is sensitive to wedge sign: flipping up↔down without reflecting flips stereo."""
    from rdkit import Chem

    smiles = "C[C@H](O)Cl"
    layout = _layout(smiles)
    wedged = [b for b in layout.bonds if b.stereo in {"up", "down"}]
    assert wedged, "expected at least one wedge"
    flipped = layout.model_copy(
        update={
            "bonds": [
                (
                    b.model_copy(
                        update={"stereo": "down" if b.stereo == "up" else "up"}
                    )
                    if b.stereo in {"up", "down"}
                    else b
                )
                for b in layout.bonds
            ]
        }
    )
    got = isomeric_smiles_from_layout(flipped)
    assert got == isomeric_smiles("C[C@@H](O)Cl")
    assert got != isomeric_smiles(smiles)
    # CIP centers must also flip.
    mol = layout_to_rdkit_with_wedges(flipped)
    centers = Chem.FindMolChiralCenters(mol)
    want_centers = Chem.FindMolChiralCenters(Chem.MolFromSmiles("C[C@@H](O)Cl"))
    assert [c[1] for c in centers] == [c[1] for c in want_centers]


def test_correctness_does_not_require_matching_rdkit_wedge_bonds() -> None:
    """Stereo oracle is isomeric SMILES — not RDKit's chosen wedge bond list.

    Build a reference RDKit wedge set independently; xpict may agree or not.
    We only assert chirality recovery, never ``wedges == rdkit_wedges``.
    """
    from rdkit import Chem
    from rdkit.Chem import AllChem, rdmolops

    smiles = "N[C@@H](Cc1ccccc1)C(=O)O"
    layout = _layout(smiles)
    _assert_stereo_correct(smiles, layout)

    rmol = Chem.MolFromSmiles(smiles)
    AllChem.Compute2DCoords(rmol)
    rdmolops.WedgeMolBonds(rmol, rmol.GetConformer())
    rdkit_wedges = []
    for bond in rmol.GetBonds():
        d = bond.GetBondDir()
        if d == Chem.BondDir.BEGINWEDGE:
            rdkit_wedges.append((bond.GetBeginAtomIdx(), bond.GetEndAtomIdx(), "up"))
        elif d == Chem.BondDir.BEGINDASH:
            rdkit_wedges.append((bond.GetBeginAtomIdx(), bond.GetEndAtomIdx(), "down"))
    xpict_wedges = [
        (b.begin, b.end, b.stereo) for b in layout.bonds if b.stereo in {"up", "down"}
    ]
    # Documented non-goal: these lists are not part of the assertion.
    del rdkit_wedges, xpict_wedges
    assert isomeric_smiles_from_layout(layout) == isomeric_smiles(smiles)


def test_multi_center_diastereomer_not_confused() -> None:
    """Both centers matter: like and unlike diastereomers stay distinct."""
    like = "C[C@H](O)[C@H](C)O"
    unlike = "C[C@H](O)[C@@H](C)O"
    assert isomeric_smiles(like) != isomeric_smiles(unlike)
    assert isomeric_smiles_from_layout(_layout(like)) == isomeric_smiles(like)
    assert isomeric_smiles_from_layout(_layout(unlike)) == isomeric_smiles(unlike)


def test_achiral_has_no_tetrahedral_wedges_required() -> None:
    """Achiral molecules need not wedge; rebuild still matches (no stereo)."""
    smiles = "CCO"
    layout = _layout(smiles)
    assert isomeric_smiles_from_layout(layout) == isomeric_smiles(smiles)
    assert not any(b.stereo in {"up", "down"} for b in layout.bonds)


def test_layout_to_rdkit_reorients_bond_begin_to_stereocenter() -> None:
    """BondDir begin must be the stereocenter even if AddBond stored the reverse."""
    from rdkit import Chem

    # Minimal hand layout: C(stereocenter)-O wedge up, plus C and Cl.
    # Indices: 0=Me, 1=C*, 2=O, 3=Cl
    from xpict.contracts.layout import AtomLayout

    layout = MoleculeLayout(
        atoms=[
            AtomLayout(index=0, element="C", x=0.0, y=0.0),
            AtomLayout(index=1, element="C", x=1.0, y=0.0),
            AtomLayout(index=2, element="O", x=1.5, y=1.0),
            AtomLayout(index=3, element="Cl", x=1.5, y=-1.0),
        ],
        bonds=[
            BondLayout(index=0, begin=1, end=0, order=1.0, stereo="none"),
            BondLayout(index=1, begin=1, end=2, order=1.0, stereo="up"),
            BondLayout(index=2, begin=1, end=3, order=1.0, stereo="none"),
        ],
        backend="rdkit",
    )
    mol = layout_to_rdkit_with_wedges(layout)
    wedged = [b for b in mol.GetBonds() if b.GetBondDir() != Chem.BondDir.NONE]
    assert len(wedged) == 1
    assert wedged[0].GetBeginAtomIdx() == 1  # stereocenter


@pytest.mark.parametrize(
    "smiles",
    [
        "C[C@H](O)Cl",
        "N[C@@H](Cc1ccccc1)C(=O)O",
        "C[C@H]1CC[C@@H](O)CC1",
    ],
)
def test_cip_labels_match_reference(smiles: str) -> None:
    """CIP R/S labels after wedge perception match the SMILES molecule."""
    from rdkit import Chem

    layout = _layout(smiles)
    got_mol = layout_to_rdkit_with_wedges(layout)
    ref_mol = Chem.MolFromSmiles(smiles)
    Chem.AssignStereochemistry(ref_mol, force=True, cleanIt=True)
    got = Chem.FindMolChiralCenters(got_mol)
    ref = Chem.FindMolChiralCenters(ref_mol)
    # got_mol.GetSubstructMatch(ref) → match[ref_idx] = got_idx (order-robust).
    match = got_mol.GetSubstructMatch(ref_mol, useChirality=False)
    assert match, "connectivity mismatch between layout rebuild and SMILES"
    inv = {got_idx: ref_idx for ref_idx, got_idx in enumerate(match)}
    remapped = sorted((inv[g_i], tag) for g_i, tag in got)
    assert remapped == sorted(ref)

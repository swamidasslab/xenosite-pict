"""Smoke + constrained-align checks for native CoordGen (RDKit kept as graph source)."""

from __future__ import annotations

import math

import pytest
from rdkit import Chem

from xpict import _native
from xpict.layout2d import (
    LAYOUT_FLAG_BEST_PRECISION,
    LAYOUT_FLAG_EVEN_ANGLES,
    LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES,
    LAYOUT_FLAG_METAL_BONDS_AS_ZOB,
    LAYOUT_FLAG_QUICK_PRECISION,
    LAYOUT_FLAG_SKIP_MINIMIZATION,
    Layout2DAtom,
    Layout2DBond,
    Layout2DRequest,
    Layout2DResult,
    generate_2d_coordinates,
)


def test_has_coordgen_linked():
    assert _native.HAS_COORDGEN is True


def test_native_flags_exist():
    assert _native.LAYOUT_FLAG_EVEN_ANGLES == LAYOUT_FLAG_EVEN_ANGLES
    assert _native.LAYOUT_FLAG_QUICK_PRECISION == LAYOUT_FLAG_QUICK_PRECISION
    assert hasattr(_native, "LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES")
    assert hasattr(_native, "LAYOUT_FLAG_BEST_PRECISION")
    assert hasattr(_native, "LAYOUT_FLAG_METAL_BONDS_AS_ZOB")
    assert hasattr(_native, "LAYOUT_FLAG_SKIP_MINIMIZATION")


def _rdkit_to_request(
    smiles: str,
    *,
    layout_flags: int = 0,
    constrained: dict[int, tuple[float, float]] | None = None,
    fixed: dict[int, tuple[float, float]] | None = None,
) -> Layout2DRequest:
    """Test-only helper: RDKit parse → Layout2DRequest (not a public API)."""
    mol = Chem.MolFromSmiles(smiles)
    assert mol is not None, smiles
    try:
        Chem.Kekulize(mol, clearAromaticFlags=True)
    except Chem.KekulizeException:
        pass
    mol = Chem.RemoveHs(mol)

    atoms: list[Layout2DAtom] = []
    for atom in mol.GetAtoms():
        i = atom.GetIdx()
        kw: dict = {
            "atomic_number": atom.GetAtomicNum(),
            "formal_charge": atom.GetFormalCharge(),
        }
        if fixed and i in fixed:
            x, y = fixed[i]
            kw.update(fixed=True, x=x, y=y)
        elif constrained and i in constrained:
            x, y = constrained[i]
            kw.update(constrained=True, x=x, y=y)
        atoms.append(Layout2DAtom(**kw))

    bonds: list[Layout2DBond] = []
    for bond in mol.GetBonds():
        order = int(round(bond.GetBondTypeAsDouble()))
        order = max(1, min(3, order)) if order else 1
        bkw: dict = {
            "begin": bond.GetBeginAtomIdx(),
            "end": bond.GetEndAtomIdx(),
            "order": order,
        }
        st = bond.GetStereo()
        stereo_atoms = list(bond.GetStereoAtoms())
        if len(stereo_atoms) == 2 and st in (
            Chem.BondStereo.STEREOZ,
            Chem.BondStereo.STEREOCIS,
            Chem.BondStereo.STEREOE,
            Chem.BondStereo.STEREOTRANS,
        ):
            bkw["stereo"] = (
                "cis"
                if st in (Chem.BondStereo.STEREOZ, Chem.BondStereo.STEREOCIS)
                else "trans"
            )
            bkw["stereo_atom1"] = int(stereo_atoms[0])
            bkw["stereo_atom2"] = int(stereo_atoms[1])
        bonds.append(Layout2DBond(**bkw))
    return Layout2DRequest(atoms=atoms, bonds=bonds, layout_flags=layout_flags)


def _same_side(
    res: Layout2DResult, begin: int, end: int, s1: int, s2: int
) -> bool:
    ax, ay = res.x[begin], res.y[begin]
    bx, by = res.x[end], res.y[end]
    cross1 = (bx - ax) * (res.y[s1] - ay) - (by - ay) * (res.x[s1] - ax)
    cross2 = (bx - ax) * (res.y[s2] - by) - (by - ay) * (res.x[s2] - bx)
    return cross1 * cross2 > 0.0


def _assert_finite(res: Layout2DResult, n: int) -> None:
    assert len(res.x) == n
    assert len(res.y) == n
    assert all(math.isfinite(v) for v in res.x + res.y)
    assert any(abs(x) > 1e-3 or abs(y) > 1e-3 for x, y in res.coords)


def test_generate_2d_coordinates_ethanol():
    res = generate_2d_coordinates([6, 6, 8], [0, 1], [1, 2], [1, 1])
    _assert_finite(res, 3)


def test_free_layout_benzene_and_flags():
    for flags in (
        0,
        LAYOUT_FLAG_QUICK_PRECISION,
        LAYOUT_FLAG_BEST_PRECISION,
        LAYOUT_FLAG_EVEN_ANGLES,
        LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES,
        LAYOUT_FLAG_QUICK_PRECISION | LAYOUT_FLAG_EVEN_ANGLES,
    ):
        req = _rdkit_to_request("c1ccccc1", layout_flags=flags)
        res = generate_2d_coordinates(req)
        _assert_finite(res, len(req.atoms))


def test_free_layout_charged_atom():
    # Multi-atom charged species (single-atom layouts sit at origin).
    req = _rdkit_to_request("C[NH3+]")
    assert any(a.formal_charge != 0 for a in req.atoms)
    res = generate_2d_coordinates(req)
    _assert_finite(res, len(req.atoms))


def test_typed_request_with_fixed_atom_and_flags():
    req = Layout2DRequest(
        atoms=[
            Layout2DAtom(atomic_number=6, fixed=True, x=10.0, y=20.0),
            Layout2DAtom(atomic_number=6),
            Layout2DAtom(atomic_number=8),
        ],
        bonds=[
            Layout2DBond(begin=0, end=1, order=1),
            Layout2DBond(begin=1, end=2, order=1),
        ],
        layout_flags=LAYOUT_FLAG_QUICK_PRECISION | LAYOUT_FLAG_EVEN_ANGLES,
    )
    res = generate_2d_coordinates(req)
    assert len(res.x) == 3
    assert abs(res.x[0] - 10.0) < 5.0
    assert abs(res.y[0] - 20.0) < 5.0


def test_constrained_phenol_anisole_ring():
    """Lay out phenol free; constrain anisole ring atoms to phenol coords."""
    phenol_req = _rdkit_to_request("c1ccccc1O")
    phenol_res = generate_2d_coordinates(phenol_req)
    _assert_finite(phenol_res, len(phenol_req.atoms))

    phenol_g = Chem.RemoveHs(Chem.MolFromSmiles("c1ccccc1O"))
    anisole_g = Chem.RemoveHs(Chem.MolFromSmiles("COc1ccccc1"))
    assert phenol_g is not None and anisole_g is not None
    ring = Chem.MolFromSmarts("c1ccccc1")
    assert ring is not None
    p_ring = phenol_g.GetSubstructMatch(ring)
    a_ring = anisole_g.GetSubstructMatch(ring)
    assert len(p_ring) == 6 and len(a_ring) == 6

    constrained = {
        int(a_idx): (phenol_res.x[int(p_idx)], phenol_res.y[int(p_idx)])
        for a_idx, p_idx in zip(a_ring, p_ring, strict=True)
    }
    anisole_req = _rdkit_to_request("COc1ccccc1", constrained=constrained)
    anisole_res = generate_2d_coordinates(anisole_req)
    _assert_finite(anisole_res, len(anisole_req.atoms))

    for a_idx, (sx, sy) in constrained.items():
        assert abs(anisole_res.x[a_idx] - sx) < 5.0, (a_idx, anisole_res.x[a_idx], sx)
        assert abs(anisole_res.y[a_idx] - sy) < 5.0, (a_idx, anisole_res.y[a_idx], sy)


def test_stereo_with_neighbor_atoms():
    req = Layout2DRequest(
        atoms=[
            Layout2DAtom(atomic_number=6),
            Layout2DAtom(atomic_number=6),
            Layout2DAtom(atomic_number=6),
            Layout2DAtom(atomic_number=6),
        ],
        bonds=[
            Layout2DBond(
                begin=0,
                end=1,
                order=2,
                stereo="cis",
                stereo_atom1=2,
                stereo_atom2=3,
            ),
            Layout2DBond(begin=0, end=2, order=1),
            Layout2DBond(begin=1, end=3, order=1),
        ],
    )
    res = generate_2d_coordinates(req)
    _assert_finite(res, 4)
    assert _same_side(res, 0, 1, 2, 3)


@pytest.mark.parametrize(
    "smiles,expect_cis",
    [
        (r"C/C=C\C", True),
        (r"C/C=C/C", False),
        (r"OC(=O)/C=C\C(=O)O", True),
        (r"OC(=O)/C=C/C(=O)O", False),
        (r"Cl/C=C\Br", True),
        (r"Cl/C=C/Br", False),
    ],
)
def test_ez_stereo_from_smiles_honored(smiles: str, expect_cis: bool):
    req = _rdkit_to_request(smiles)
    stereo_bonds = [b for b in req.bonds if b.stereo in ("cis", "trans")]
    assert stereo_bonds, f"no E/Z parsed from {smiles!r}"
    res = generate_2d_coordinates(req)
    _assert_finite(res, len(req.atoms))
    for b in stereo_bonds:
        assert b.stereo_atom1 is not None and b.stereo_atom2 is not None
        same = _same_side(res, b.begin, b.end, b.stereo_atom1, b.stereo_atom2)
        if expect_cis:
            assert same, (smiles, b, res.coords)
        else:
            assert not same, (smiles, b, res.coords)


@pytest.mark.parametrize(
    "smiles",
    [
        "C[C@H](O)C(=O)O",
        "C[C@@H](N)C(=O)O",
        "C[C@H](O)[C@@H](C)NC",
        "C[C@H](O)[C@H](C)NC",  # diastereomer — layout only (no wedge API)
        r"C/C=C/[C@H](C)O",  # E alkene + chiral
    ],
)
def test_chiral_and_mixed_stereo_layout_finite(smiles: str):
    req = _rdkit_to_request(smiles)
    res = generate_2d_coordinates(req)
    _assert_finite(res, len(req.atoms))


def test_skip_minimization_and_metal_flags_do_not_error():
    req = _rdkit_to_request(
        "c1ccccc1",
        layout_flags=LAYOUT_FLAG_SKIP_MINIMIZATION | LAYOUT_FLAG_METAL_BONDS_AS_ZOB,
    )
    res = generate_2d_coordinates(req)
    _assert_finite(res, len(req.atoms))

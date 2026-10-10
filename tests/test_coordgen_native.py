"""Smoke + constrained-align checks for native CoordGen (RDKit kept as graph source)."""

from __future__ import annotations

import math
from dataclasses import dataclass, field

import pytest
from rdkit import Chem

from xpict import _native


def test_has_coordgen_linked():
    assert _native.HAS_COORDGEN is True


def test_native_flags_exist():
    assert hasattr(_native, "LAYOUT_FLAG_EVEN_ANGLES")
    assert hasattr(_native, "LAYOUT_FLAG_QUICK_PRECISION")
    assert hasattr(_native, "LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES")
    assert hasattr(_native, "LAYOUT_FLAG_BEST_PRECISION")
    assert hasattr(_native, "LAYOUT_FLAG_METAL_BONDS_AS_ZOB")
    assert hasattr(_native, "LAYOUT_FLAG_SKIP_MINIMIZATION")


@dataclass
class _Graph:
    """Test-only flat arrays for ``_native.generate_2d_coordinates``."""

    atomic_numbers: list[int]
    bond_from: list[int]
    bond_to: list[int]
    bond_orders: list[int]
    formal_charges: list[int] = field(default_factory=list)
    bond_stereos: list[int] = field(default_factory=list)
    bond_stereo_atom1: list[int] = field(default_factory=list)
    bond_stereo_atom2: list[int] = field(default_factory=list)
    has_template_match: list[int] | None = None
    is_atom_fixed: list[int] | None = None
    input_x: list[float] | None = None
    input_y: list[float] | None = None
    layout_flags: int = 0

    @property
    def n_atoms(self) -> int:
        return len(self.atomic_numbers)

    def layout(self):
        return _native.generate_2d_coordinates(
            self.atomic_numbers,
            self.bond_from,
            self.bond_to,
            self.bond_orders,
            formal_charges=self.formal_charges or None,
            bond_stereos=self.bond_stereos or None,
            bond_stereo_atom1=self.bond_stereo_atom1 or None,
            bond_stereo_atom2=self.bond_stereo_atom2 or None,
            has_template_match=self.has_template_match,
            is_atom_fixed=self.is_atom_fixed,
            input_x=self.input_x,
            input_y=self.input_y,
            layout_flags=self.layout_flags or None,
        )


def _rdkit_to_graph(
    smiles: str,
    *,
    layout_flags: int = 0,
    constrained: dict[int, tuple[float, float]] | None = None,
    fixed: dict[int, tuple[float, float]] | None = None,
) -> _Graph:
    """Test-only helper: RDKit parse → flat native layout args."""
    mol = Chem.MolFromSmiles(smiles)
    assert mol is not None, smiles
    try:
        Chem.Kekulize(mol, clearAromaticFlags=True)
    except Chem.KekulizeException:
        pass
    mol = Chem.RemoveHs(mol)

    n = mol.GetNumAtoms()
    atomic_numbers = [0] * n
    formal_charges = [0] * n
    has_template_match = [0] * n
    is_atom_fixed = [0] * n
    input_x = [0.0] * n
    input_y = [0.0] * n
    need_input = False

    for atom in mol.GetAtoms():
        i = atom.GetIdx()
        atomic_numbers[i] = atom.GetAtomicNum()
        formal_charges[i] = atom.GetFormalCharge()
        if fixed and i in fixed:
            x, y = fixed[i]
            is_atom_fixed[i] = 1
            input_x[i], input_y[i] = x, y
            need_input = True
        elif constrained and i in constrained:
            x, y = constrained[i]
            has_template_match[i] = 1
            input_x[i], input_y[i] = x, y
            need_input = True

    bond_from: list[int] = []
    bond_to: list[int] = []
    bond_orders: list[int] = []
    bond_stereos: list[int] = []
    bond_stereo_atom1: list[int] = []
    bond_stereo_atom2: list[int] = []
    for bond in mol.GetBonds():
        order = int(round(bond.GetBondTypeAsDouble()))
        order = max(1, min(3, order)) if order else 1
        bond_from.append(bond.GetBeginAtomIdx())
        bond_to.append(bond.GetEndAtomIdx())
        bond_orders.append(order)
        st = bond.GetStereo()
        stereo_atoms = list(bond.GetStereoAtoms())
        if len(stereo_atoms) == 2 and st in (
            Chem.BondStereo.STEREOZ,
            Chem.BondStereo.STEREOCIS,
            Chem.BondStereo.STEREOE,
            Chem.BondStereo.STEREOTRANS,
        ):
            cis = st in (Chem.BondStereo.STEREOZ, Chem.BondStereo.STEREOCIS)
            bond_stereos.append(
                _native.BOND_STEREO_CIS if cis else _native.BOND_STEREO_TRANS
            )
            bond_stereo_atom1.append(int(stereo_atoms[0]))
            bond_stereo_atom2.append(int(stereo_atoms[1]))
        else:
            bond_stereos.append(_native.BOND_STEREO_NONE)
            bond_stereo_atom1.append(-1)
            bond_stereo_atom2.append(-1)

    return _Graph(
        atomic_numbers=atomic_numbers,
        bond_from=bond_from,
        bond_to=bond_to,
        bond_orders=bond_orders,
        formal_charges=formal_charges,
        bond_stereos=bond_stereos,
        bond_stereo_atom1=bond_stereo_atom1,
        bond_stereo_atom2=bond_stereo_atom2,
        has_template_match=has_template_match if need_input else None,
        is_atom_fixed=is_atom_fixed if need_input else None,
        input_x=input_x if need_input else None,
        input_y=input_y if need_input else None,
        layout_flags=layout_flags,
    )


def _same_side(res, begin: int, end: int, s1: int, s2: int) -> bool:
    ax, ay = res.x[begin], res.y[begin]
    bx, by = res.x[end], res.y[end]
    cross1 = (bx - ax) * (res.y[s1] - ay) - (by - ay) * (res.x[s1] - ax)
    cross2 = (bx - ax) * (res.y[s2] - by) - (by - ay) * (res.x[s2] - bx)
    return cross1 * cross2 > 0.0


def _assert_finite(res, n: int) -> None:
    assert len(res.x) == n
    assert len(res.y) == n
    assert all(math.isfinite(v) for v in list(res.x) + list(res.y))
    coords = res.coords()
    assert any(abs(x) > 1e-3 or abs(y) > 1e-3 for x, y in coords)


def test_generate_2d_coordinates_ethanol():
    res = _native.generate_2d_coordinates([6, 6, 8], [0, 1], [1, 2], [1, 1])
    _assert_finite(res, 3)


def test_free_layout_benzene_and_flags():
    for flags in (
        0,
        _native.LAYOUT_FLAG_QUICK_PRECISION,
        _native.LAYOUT_FLAG_BEST_PRECISION,
        _native.LAYOUT_FLAG_EVEN_ANGLES,
        _native.LAYOUT_FLAG_FORCE_OPEN_MACROCYCLES,
        _native.LAYOUT_FLAG_QUICK_PRECISION | _native.LAYOUT_FLAG_EVEN_ANGLES,
    ):
        g = _rdkit_to_graph("c1ccccc1", layout_flags=flags)
        res = g.layout()
        _assert_finite(res, g.n_atoms)


def test_free_layout_charged_atom():
    # Multi-atom charged species (single-atom layouts sit at origin).
    g = _rdkit_to_graph("C[NH3+]")
    assert any(c != 0 for c in g.formal_charges)
    res = g.layout()
    _assert_finite(res, g.n_atoms)


def test_fixed_atom_and_flags():
    g = _Graph(
        atomic_numbers=[6, 6, 8],
        bond_from=[0, 1],
        bond_to=[1, 2],
        bond_orders=[1, 1],
        is_atom_fixed=[1, 0, 0],
        input_x=[10.0, 0.0, 0.0],
        input_y=[20.0, 0.0, 0.0],
        layout_flags=_native.LAYOUT_FLAG_QUICK_PRECISION | _native.LAYOUT_FLAG_EVEN_ANGLES,
    )
    res = g.layout()
    assert len(res.x) == 3
    assert abs(res.x[0] - 10.0) < 5.0
    assert abs(res.y[0] - 20.0) < 5.0


def test_constrained_phenol_anisole_ring():
    """Lay out phenol free; constrain anisole ring atoms to phenol coords."""
    phenol = _rdkit_to_graph("c1ccccc1O")
    phenol_res = phenol.layout()
    _assert_finite(phenol_res, phenol.n_atoms)

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
    anisole = _rdkit_to_graph("COc1ccccc1", constrained=constrained)
    anisole_res = anisole.layout()
    _assert_finite(anisole_res, anisole.n_atoms)

    for a_idx, (sx, sy) in constrained.items():
        assert abs(anisole_res.x[a_idx] - sx) < 5.0, (a_idx, anisole_res.x[a_idx], sx)
        assert abs(anisole_res.y[a_idx] - sy) < 5.0, (a_idx, anisole_res.y[a_idx], sy)


def test_stereo_with_neighbor_atoms():
    g = _Graph(
        atomic_numbers=[6, 6, 6, 6],
        bond_from=[0, 0, 1],
        bond_to=[1, 2, 3],
        bond_orders=[2, 1, 1],
        bond_stereos=[
            _native.BOND_STEREO_CIS,
            _native.BOND_STEREO_NONE,
            _native.BOND_STEREO_NONE,
        ],
        bond_stereo_atom1=[2, -1, -1],
        bond_stereo_atom2=[3, -1, -1],
    )
    res = g.layout()
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
    g = _rdkit_to_graph(smiles)
    stereo_ix = [
        i
        for i, st in enumerate(g.bond_stereos)
        if st in (_native.BOND_STEREO_CIS, _native.BOND_STEREO_TRANS)
    ]
    assert stereo_ix, f"no E/Z parsed from {smiles!r}"
    res = g.layout()
    _assert_finite(res, g.n_atoms)
    for i in stereo_ix:
        s1, s2 = g.bond_stereo_atom1[i], g.bond_stereo_atom2[i]
        assert s1 >= 0 and s2 >= 0
        same = _same_side(res, g.bond_from[i], g.bond_to[i], s1, s2)
        if expect_cis:
            assert same, (smiles, i, res.coords())
        else:
            assert not same, (smiles, i, res.coords())


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
    g = _rdkit_to_graph(smiles)
    res = g.layout()
    _assert_finite(res, g.n_atoms)


def test_skip_minimization_and_metal_flags_do_not_error():
    g = _rdkit_to_graph(
        "c1ccccc1",
        layout_flags=_native.LAYOUT_FLAG_SKIP_MINIMIZATION
        | _native.LAYOUT_FLAG_METAL_BONDS_AS_ZOB,
    )
    res = g.layout()
    _assert_finite(res, g.n_atoms)

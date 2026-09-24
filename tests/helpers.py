"""Shared test helpers (importable; not pytest fixtures)."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

if TYPE_CHECKING:
    from xpict.contracts.layout import MoleculeLayout


def layout_backend() -> str:
    """RDKit layout backend; skip the test when RDKit is not installed."""
    pytest.importorskip("rdkit")
    return "rdkit"


def isomeric_smiles(source: str) -> str:
    """Canonical isomeric SMILES for ``source`` (stereo ground truth)."""
    from rdkit import Chem

    mol = Chem.MolFromSmiles(source)
    if mol is None:
        raise ValueError(f"RDKit could not parse {source!r}")
    Chem.AssignStereochemistry(mol, force=True, cleanIt=True)
    return Chem.MolToSmiles(mol, isomericSmiles=True)


def layout_to_rdkit_with_wedges(layout: MoleculeLayout) -> Any:
    """Rebuild an RDKit mol from layout coords + ``up``/``down`` bond dirs.

    BondDir begin atom is the stereocenter (``BondLayout.begin``), matching
    RDKit ``WedgeMolBonds`` / our draw convention. Does **not** compare which
    bonds were wedged — callers use chirality assignment as the oracle.
    """
    from rdkit import Chem
    from rdkit.Geometry import Point3D

    em = Chem.EditableMol(Chem.Mol())
    idx: dict[int, int] = {}
    for atom in sorted(layout.atoms, key=lambda a: a.index):
        a = Chem.Atom(atom.element)
        if atom.charge:
            a.SetFormalCharge(int(atom.charge))
        idx[atom.index] = em.AddAtom(a)
    for bond in layout.bonds:
        if bond.order >= 2.5:
            kind = Chem.BondType.TRIPLE
        elif bond.order >= 1.5:
            kind = Chem.BondType.DOUBLE
        else:
            kind = Chem.BondType.SINGLE
        em.AddBond(idx[bond.begin], idx[bond.end], kind)
    mol = em.GetMol()
    Chem.SanitizeMol(mol)
    conf = Chem.Conformer(mol.GetNumAtoms())
    for atom in layout.atoms:
        conf.SetAtomPosition(idx[atom.index], Point3D(float(atom.x), float(atom.y), 0.0))
    mol.AddConformer(conf)

    for bond in layout.bonds:
        if bond.stereo not in {"up", "down"}:
            continue
        begin_i = idx[bond.begin]
        end_i = idx[bond.end]
        rb = mol.GetBondBetweenAtoms(begin_i, end_i)
        if rb is None:
            raise AssertionError(f"missing bond {bond.begin}-{bond.end}")
        if rb.GetBeginAtomIdx() != begin_i:
            rb.SetBeginAtomIdx(begin_i)
            rb.SetEndAtomIdx(end_i)
        rb.SetBondDir(
            Chem.BondDir.BEGINWEDGE if bond.stereo == "up" else Chem.BondDir.BEGINDASH
        )

    Chem.AssignChiralTypesFromBondDirs(mol)
    Chem.AssignStereochemistry(mol, force=True, cleanIt=True)
    return mol


def isomeric_smiles_from_layout(layout: MoleculeLayout) -> str:
    """Isomeric SMILES implied by layout wedges + 2D coords (RDKit perception)."""
    from rdkit import Chem

    mol = layout_to_rdkit_with_wedges(layout)
    return Chem.MolToSmiles(mol, isomericSmiles=True)


def random_isomeric_smiles(source: str, *, n: int = 24) -> list[str]:
    """Distinct random isomeric SMILES writings of the same stereo molecule."""
    from rdkit import Chem

    mol = Chem.MolFromSmiles(source)
    if mol is None:
        raise ValueError(f"RDKit could not parse {source!r}")
    out: list[str] = []
    seen: set[str] = set()
    # Deterministic attempts; RDKit's RNG is process-global — enough diversity.
    for _ in range(max(n * 4, 32)):
        s = Chem.MolToSmiles(mol, doRandom=True, isomericSmiles=True)
        if s not in seen:
            seen.add(s)
            out.append(s)
        if len(out) >= n:
            break
    if source not in seen:
        out.insert(0, source)
    return out

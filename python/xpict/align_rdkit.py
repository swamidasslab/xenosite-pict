"""RDKit template alignment.

Matched atoms stay on the reference coordinates. Everything else is
regenerated around that core (``Compute2DCoords`` + ``coordMap``), at the
reference bond length. If that fails, callers use the rigid aligner.
"""

from __future__ import annotations

import math

from xpict.align import RigidAligner, choose_mapping, with_warning
from xpict.contracts.layout import BondLayout, MoleculeLayout

_MIN_MAP = 3
_PLACE_CAP = 8
_ORDER_CAP = 24


def mcs_params():
    """FMCS: element + hybridization atoms; any-bond (aromatic ↔ kekulé / quinone).

    Hybridization separates aliphatic rings from quinones without a post-filter.
    Parity with JS MinimalLib (isotope-encoded Z×10+hyb + ``AtomCompare: Isotopes``).
    Align passes **atom matches only** into Depictor (not the MCS bond pattern).
    Shared by document align and single-mol ``client`` layout.
    """
    from rdkit.Chem import rdFMCS

    class _ElemHyb(rdFMCS.MCSAtomCompare):
        def __call__(self, _params, mol1, idx1, mol2, idx2) -> bool:  # noqa: ANN001
            a1 = mol1.GetAtomWithIdx(idx1)
            a2 = mol2.GetAtomWithIdx(idx2)
            if a1.GetAtomicNum() != a2.GetAtomicNum():
                return False
            return a1.GetHybridization() == a2.GetHybridization()

    params = rdFMCS.MCSParameters()
    params.Timeout = 2
    params.AtomTyper = _ElemHyb()
    params.BondTyper = rdFMCS.BondCompare.CompareAny
    return params


def _substruct_orders(mol, pattern) -> list[tuple[int, ...]]:
    """Match orders, with at least one order for every place the pattern sits.

    ``uniquify=True`` collapses a symmetric ring to a single arbitrary
    rotation. ``uniquify=False`` is capped, and that cap can fill up on the
    first place, so unique places are merged back in.
    """
    places = mol.GetSubstructMatches(pattern, uniquify=True, maxMatches=_PLACE_CAP)
    orders = mol.GetSubstructMatches(pattern, uniquify=False, maxMatches=_PLACE_CAP * _ORDER_CAP)
    grouped: dict[frozenset[int], list[tuple[int, ...]]] = {}
    for match in list(orders) + list(places):
        grouped.setdefault(frozenset(match), [])
        bucket = grouped[frozenset(match)]
        if match not in bucket:
            bucket.append(match)
    selected: list[tuple[int, ...]] = []
    for i, bucket in enumerate(grouped.values()):
        if i >= _PLACE_CAP:
            break
        selected.extend(bucket[:_ORDER_CAP])
    return selected


def _fmcs_mapping(ref: MoleculeLayout, other: MoleculeLayout) -> dict[int, int] | None:
    from rdkit import Chem
    from rdkit.Chem import rdFMCS

    built_ref = layout_to_rdkit(ref)
    built_other = layout_to_rdkit(other)
    if built_ref is None or built_other is None:
        return None
    ref_mol, ref_to_rd = built_ref
    other_mol, other_to_rd = built_other
    rd_to_ref = {rd: lay for lay, rd in ref_to_rd.items()}
    rd_to_other = {rd: lay for lay, rd in other_to_rd.items()}
    try:
        mcs = rdFMCS.FindMCS([ref_mol, other_mol], mcs_params())
    except Exception:
        return None
    if getattr(mcs, "canceled", False) or mcs.numAtoms < _MIN_MAP:
        return None
    try:
        pattern = Chem.MolFromSmarts(mcs.smartsString)
    except Exception:
        pattern = None
    if pattern is None:
        return None
    ref_orders = _substruct_orders(ref_mol, pattern)
    other_orders = _substruct_orders(other_mol, pattern)
    mappings: list[dict[int, int]] = []
    for ref_match in ref_orders:
        for other_match in other_orders:
            if len(ref_match) != len(other_match):
                continue
            mappings.append(
                {
                    rd_to_other[other_i]: rd_to_ref[ref_i]
                    for ref_i, other_i in zip(ref_match, other_match, strict=True)
                }
            )
    return choose_mapping(ref, other, mappings, min_size=_MIN_MAP)


def rdkit_available() -> bool:
    try:
        import importlib.util

        return importlib.util.find_spec("rdkit") is not None
    except (ImportError, ValueError, ModuleNotFoundError):
        return False


def _plain_smiles(smiles: str | None) -> str | None:
    if not smiles:
        return None
    text = smiles.split("|", 1)[0].strip()
    return text or None


def _bond_type(order: float):
    from rdkit import Chem

    if order >= 2.5 and order < 3.5:
        return Chem.BondType.TRIPLE
    if order >= 1.5 and order < 2.5:
        return Chem.BondType.DOUBLE
    if order >= 3.5:
        return Chem.BondType.AROMATIC
    return Chem.BondType.SINGLE


def _rd_atom(element: str, charge: int, isotope: int | None):
    from rdkit import Chem

    if element in {"*", "R"} or element.startswith("R") or element.startswith("_"):
        atom = Chem.Atom(0)
    else:
        atom = Chem.Atom(element)
    if charge:
        atom.SetFormalCharge(charge)
    if isotope:
        atom.SetIsotope(isotope)
    return atom


def layout_to_rdkit(layout: MoleculeLayout):
    """Build an RDKit mol whose atom order maps back to layout indices.

    Returns ``(mol, layout_index_to_rdkit_index)`` or ``None``.
    """
    from rdkit import Chem

    editable = Chem.EditableMol(Chem.Mol())
    to_rd: dict[int, int] = {}
    try:
        for atom in sorted(layout.atoms, key=lambda a: a.index):
            to_rd[atom.index] = editable.AddAtom(_rd_atom(atom.element, atom.charge, atom.isotope))
        for bond in layout.bonds:
            if bond.begin not in to_rd or bond.end not in to_rd:
                continue
            editable.AddBond(to_rd[bond.begin], to_rd[bond.end], _bond_type(bond.order))
        mol = editable.GetMol()
        Chem.SanitizeMol(mol)
    except Exception:
        return None
    conformer = Chem.Conformer(mol.GetNumAtoms())
    for atom in layout.atoms:
        conformer.SetAtomPosition(to_rd[atom.index], (atom.x, atom.y, 0.0))
    mol.AddConformer(conformer, assignId=True)
    return mol, to_rd


def _copy_chirality(mol, smiles: str | None) -> bool:
    """Copy tetrahedral tags from SMILES so wedges can be rebuilt after redraw."""
    from rdkit import Chem

    text = _plain_smiles(smiles)
    if not text:
        return False
    try:
        query = Chem.MolFromSmiles(text)
    except Exception:
        query = None
    if query is None or query.GetNumAtoms() != mol.GetNumAtoms():
        return False
    match = mol.GetSubstructMatch(query)
    if len(match) != mol.GetNumAtoms():
        return False
    for query_i, layout_i in enumerate(match):
        mol.GetAtomWithIdx(layout_i).SetChiralTag(query.GetAtomWithIdx(query_i).GetChiralTag())
    return True


def _bonds_after_depict(mol, bonds: list[BondLayout], to_rd: dict[int, int], smiles: str | None):
    from rdkit import Chem

    applied = _copy_chirality(mol, smiles)
    if applied:
        try:
            Chem.WedgeMolBonds(mol, mol.GetConformer())
        except Exception:
            applied = False
    if not applied:
        return [
            bond.model_copy(update={"stereo": "none"}) if bond.stereo in {"up", "down"} else bond
            for bond in bonds
        ]
    rd_to_layout = {rd: lay for lay, rd in to_rd.items()}
    wedged: dict[frozenset[int], tuple[int, int, str]] = {}
    for bond in mol.GetBonds():
        direction = bond.GetBondDir()
        if direction == Chem.BondDir.BEGINWEDGE:
            stereo = "up"
        elif direction == Chem.BondDir.BEGINDASH:
            stereo = "down"
        else:
            continue
        begin = rd_to_layout[bond.GetBeginAtomIdx()]
        end = rd_to_layout[bond.GetEndAtomIdx()]
        wedged[frozenset((begin, end))] = (begin, end, stereo)
    out: list[BondLayout] = []
    for bond in bonds:
        update = wedged.get(frozenset((bond.begin, bond.end)))
        if update is not None:
            begin, end, stereo = update
            out.append(bond.model_copy(update={"begin": begin, "end": end, "stereo": stereo}))
        elif bond.stereo in {"up", "down"}:
            out.append(bond.model_copy(update={"stereo": "none"}))
        else:
            out.append(bond)
    return out


class RdkitAligner(RigidAligner):
    """Template depiction via RDKit ``GenerateDepictionMatching2DStructure``.

    MCS (element + hybridization, ``BondCompare.CompareAny``) finds the atom
    correspondence; Depictor is called with that **atom map only** — MCS bond
    topology is not passed as a reference pattern. No element-only MCS
    fallback. The reference layout / pose mol is never modified.
    """

    name = "rdkit"
    supports_template = True

    def map_atoms(self, ref: MoleculeLayout, other: MoleculeLayout) -> dict[int, int] | None:
        return _fmcs_mapping(ref, other)

    def depict_on_template(
        self,
        ref: MoleculeLayout,
        other: MoleculeLayout,
        mapping: dict[int, int],
        *,
        smiles: str | None = None,
    ) -> MoleculeLayout | None:
        from rdkit import Chem
        from rdkit.Chem import rdDepictor

        built_ref = layout_to_rdkit(ref)
        built_other = layout_to_rdkit(other)
        if built_ref is None or built_other is None:
            return None
        ref_mol, ref_to_rd = built_ref
        other_mol, to_rd = built_other
        if len(mapping) < _MIN_MAP:
            return None
        # (reference_idx, query_idx) — atom matches only; no MCS bond pattern.
        atom_map = [
            (ref_to_rd[ref_i], to_rd[other_i])
            for other_i, ref_i in mapping.items()
            if other_i in to_rd and ref_i in ref_to_rd
        ]
        if len(atom_map) < _MIN_MAP:
            return None
        # Copy so Depictor cannot touch the caller's reference pose.
        ref_pose = Chem.Mol(ref_mol)
        ref_before = [
            (ref_pose.GetConformer().GetAtomPosition(i).x,
             ref_pose.GetConformer().GetAtomPosition(i).y)
            for i in range(ref_pose.GetNumAtoms())
        ]

        params = rdDepictor.ConstrainedDepictionParams()
        params.allowRGroups = True
        params.acceptFailure = False
        try:
            rdDepictor.GenerateDepictionMatching2DStructure(
                other_mol, ref_pose, atom_map, -1, params
            )
        except Exception:
            return None

        ref_after = [
            (ref_pose.GetConformer().GetAtomPosition(i).x,
             ref_pose.GetConformer().GetAtomPosition(i).y)
            for i in range(ref_pose.GetNumAtoms())
        ]
        if any(
            math.hypot(ax - bx, ay - by) > 1e-9
            for (ax, ay), (bx, by) in zip(ref_before, ref_after, strict=True)
        ):
            return None

        conformer = other_mol.GetConformer()
        atoms = [
            atom.model_copy(
                update={
                    "x": float(conformer.GetAtomPosition(to_rd[atom.index]).x),
                    "y": float(conformer.GetAtomPosition(to_rd[atom.index]).y),
                }
            )
            for atom in other.atoms
        ]
        bonds = _bonds_after_depict(other_mol, other.bonds, to_rd, smiles)
        laid = other.model_copy(update={"atoms": atoms, "bonds": bonds})
        return with_warning(laid, "alignment: rdkit template depiction")

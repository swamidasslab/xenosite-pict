"""RDKit layout backend — primary focus for 2D coords (language-edge RDKit).

Uses ``Compute2DCoords`` / MolFromSmiles. Alignment stays in ``align_rdkit``
(template depict). RDKit is **not** linked into ``xpict-core``; this module
calls the Python ``rdkit`` package only. JS will call ``@rdkit/rdkit`` the
same way and feed coords into shared Rust draw/align helpers.
"""

from __future__ import annotations

from typing import Literal

from xpict.backends.base import register, warn_unsupported
from xpict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xpict.contracts.spec import MoleculeSpec
from xpict.structure import cx_atom_labels, structure_smiles

BondStereo = Literal["up", "down", "either", "none"]


def _element_label(
    element: str,
    hcount: int,
    *,
    charge: int,
    radical: int,
    isotope: int | None,
) -> str | None:
    """Terminal-hetero style labels: ``OH``, ``NH2``; hide plain C."""
    if element == "C" and not charge and not radical and not isotope and hcount <= 4:
        return None
    if element in {"*", "R"} or element.startswith("R") or element.startswith("_"):
        return "*"
    if hcount <= 0:
        text = element
    elif hcount == 1:
        text = f"{element}H"
    else:
        text = f"{element}H{hcount}"
    if isotope:
        text = f"{isotope}{text}"
    if charge:
        sign = "+" if charge > 0 else "−"
        mag = abs(charge)
        text = f"{text}{sign}" if mag == 1 else f"{text}{mag}{sign}"
    if radical:
        # Radical shown as dots elsewhere; keep element/H/charge on the label.
        pass
    return text


def _bond_stereo(bond) -> BondStereo:
    from rdkit import Chem

    # Direction relative to begin→end as stored on the bond.
    if bond.GetBondDir() == Chem.BondDir.BEGINWEDGE:
        return "up"
    if bond.GetBondDir() == Chem.BondDir.BEGINDASH:
        return "down"
    if bond.GetBondDir() == Chem.BondDir.UNKNOWN:
        return "either"
    if bond.GetStereo() == Chem.BondStereo.STEREOANY:
        return "either"
    return "none"


def _bond_order(bond) -> float:
    from rdkit import Chem

    t = bond.GetBondType()
    if t == Chem.BondType.TRIPLE:
        return 3.0
    if t == Chem.BondType.DOUBLE:
        return 2.0
    if t == Chem.BondType.AROMATIC:
        return 1.5
    return 1.0


@register("rdkit")
class RdkitBackend:
    name = "rdkit"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        try:
            from rdkit import Chem
            from rdkit.Chem import rdDepictor
        except ImportError as e:
            raise ImportError(
                "RDKit backend requires rdkit. Install with: pip install 'xpict[rdkit]'"
            ) from e

        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES before <sep> only for now.")

        cx_labels: list[str | None] = []
        if mol.cxsmiles:
            rmol = Chem.MolFromSmiles(mol.cxsmiles)
            cx_labels = cx_atom_labels(mol.cxsmiles)
        elif mol.molfile:
            rmol = Chem.MolFromMolBlock(mol.molfile, sanitize=True, removeHs=False)
        else:
            smiles = structure_smiles(mol)
            if not smiles:
                raise ValueError("rdkit backend requires smiles, cxsmiles, esmiles, or molfile")
            rmol = Chem.MolFromSmiles(smiles)
            cx_labels = cx_atom_labels(smiles)

        if rmol is None:  # pyright: ignore[reportUnnecessaryComparison]
            raise ValueError("RDKit could not parse molecule")

        try:
            Chem.Kekulize(rmol, clearAromaticFlags=True)
        except Exception:
            pass

        rdDepictor.Compute2DCoords(rmol)
        try:
            from rdkit.Chem.rdmolops import WedgeMolBonds

            conf = rmol.GetConformer()
            WedgeMolBonds(rmol, conf)
        except Exception:
            pass

        conf = rmol.GetConformer()
        atoms: list[AtomLayout] = []
        for atom in rmol.GetAtoms():
            idx = atom.GetIdx()
            pos = conf.GetAtomPosition(idx)
            el = atom.GetSymbol()
            charge = int(atom.GetFormalCharge())
            isotope = int(atom.GetIsotope()) or None
            radical = int(atom.GetNumRadicalElectrons())
            hcount = int(atom.GetTotalNumHs())
            is_star = el == "*" or atom.GetAtomicNum() == 0
            cx = cx_labels[idx] if idx < len(cx_labels) else None

            if is_star:
                element = "*"
                label = cx or "*"
            else:
                element = el
                label = cx or _element_label(
                    el, hcount, charge=charge, radical=radical, isotope=isotope
                )

            atoms.append(
                AtomLayout(
                    index=idx,
                    element=element,
                    x=float(pos.x),
                    y=float(pos.y),
                    charge=charge,
                    isotope=isotope,
                    radical=radical,
                    label=label,
                )
            )

        bonds: list[BondLayout] = []
        for bond in rmol.GetBonds():
            begin = bond.GetBeginAtomIdx()
            end = bond.GetEndAtomIdx()
            # WedgeMolBonds: stereo direction is from the stereocenter (begin of wedge).
            stereo = _bond_stereo(bond)
            bonds.append(
                BondLayout(
                    index=bond.GetIdx(),
                    begin=begin,
                    end=end,
                    order=_bond_order(bond),
                    stereo=stereo,
                )
            )
        return MoleculeLayout(id=mol.id, atoms=atoms, bonds=bonds, backend=self.name)

"""Indigo layout backend (preferred when installed)."""

from __future__ import annotations

from xenosite.pict.backends.base import register, warn_unsupported
from xenosite.pict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec
from xenosite.pict.structure import structure_smiles


@register("indigo")
class IndigoBackend:
    name = "indigo"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        try:
            from indigo import Indigo
        except ImportError as e:
            raise ImportError(
                "Indigo backend requires epam.indigo. "
                "Install with: pip install 'xenosite-pict[indigo]'"
            ) from e

        indigo = Indigo()
        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES before <sep> only for now.")
        if mol.cxsmiles:
            # Indigo accepts many CXSMILES features; unsupported ones warn at runtime.
            imol = indigo.loadMolecule(mol.cxsmiles)
        elif mol.molfile:
            imol = indigo.loadMolecule(mol.molfile)
        else:
            smiles = structure_smiles(mol)
            if not smiles:
                raise ValueError(
                    "indigo backend requires smiles, cxsmiles, esmiles, or molfile"
                )
            imol = indigo.loadMolecule(smiles)

        imol.layout()
        # Kekulé bond orders for depiction (Indigo aromatic order is 4).
        try:
            imol.dearomatize()
        except Exception:
            pass
        atoms: list[AtomLayout] = []
        for atom in imol.iterateAtoms():
            x, y, _z = atom.xyz()
            el = atom.symbol()
            charge = atom.charge()
            label = None if el == "C" and charge == 0 else el
            atoms.append(
                AtomLayout(
                    index=atom.index(),
                    element=el,
                    x=float(x),
                    y=float(y),
                    charge=int(charge),
                    label=label,
                )
            )
        bonds: list[BondLayout] = []
        for bond in imol.iterateBonds():
            bonds.append(
                BondLayout(
                    index=bond.index(),
                    begin=bond.source().index(),
                    end=bond.destination().index(),
                    order=float(bond.bondOrder()),
                )
            )
        return MoleculeLayout(id=mol.id, atoms=atoms, bonds=bonds, backend=self.name)

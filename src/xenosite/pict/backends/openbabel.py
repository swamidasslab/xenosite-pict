"""Open Babel / pybel layout backend."""

from __future__ import annotations

from xenosite.pict.backends.base import register, warn_unsupported
from xenosite.pict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec
from xenosite.pict.structure import structure_smiles


@register("openbabel")
@register("pybel")
class OpenBabelBackend:
    """2D layout via Open Babel (pybel)."""

    name = "openbabel"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        try:
            from openbabel import openbabel as ob
            from openbabel import pybel
        except ImportError as e:
            raise ImportError(
                "Open Babel backend requires openbabel Python bindings (pybel). "
                "Install with: pip install 'xenosite-pict[openbabel]'"
            ) from e

        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES before <sep> only.")
        if mol.cxsmiles:
            warn_unsupported(
                self.name,
                "cxsmiles",
                "Open Babel may ignore ChemAxon extensions; stripping after '|'.",
            )

        if mol.molfile:
            pmol = pybel.readstring("mol", mol.molfile)
        else:
            raw = structure_smiles(mol)
            if not raw:
                raise ValueError(
                    "openbabel backend requires smiles, cxsmiles, esmiles, or molfile"
                )
            smiles = raw.split("|", 1)[0].strip()
            pmol = pybel.readstring("smi", smiles)

        pmol.make2D()
        obmol = pmol.OBMol

        atoms: list[AtomLayout] = []
        for i, atom in enumerate(ob.OBMolAtomIter(obmol)):
            el = ob.GetSymbol(atom.GetAtomicNum())
            charge = int(atom.GetFormalCharge())
            label = None if el == "C" and charge == 0 else el
            atoms.append(
                AtomLayout(
                    index=i,
                    element=el,
                    x=float(atom.GetX()),
                    y=float(atom.GetY()),
                    charge=charge,
                    label=label,
                )
            )

        bonds: list[BondLayout] = []
        for bi, bond in enumerate(ob.OBMolBondIter(obmol)):
            bonds.append(
                BondLayout(
                    index=bi,
                    begin=bond.GetBeginAtomIdx() - 1,
                    end=bond.GetEndAtomIdx() - 1,
                    order=float(bond.GetBondOrder()),
                )
            )
        return MoleculeLayout(id=mol.id, atoms=atoms, bonds=bonds, backend=self.name)

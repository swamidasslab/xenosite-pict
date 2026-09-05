"""RDKit layout backend (optional extra). Supports SMILES and CXSMILES."""

from __future__ import annotations

from xenosite.pict.backends.base import register, warn_unsupported
from xenosite.pict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec
from xenosite.pict.structure import structure_smiles


@register("rdkit")
class RdkitBackend:
    name = "rdkit"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        try:
            from rdkit import Chem
            from rdkit.Chem import AllChem
        except ImportError as e:
            raise ImportError(
                "RDKit backend requires rdkit. Install with: pip install 'xenosite-pict[rdkit]'"
            ) from e

        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES before <sep> only.")

        rw = None
        if mol.molfile:
            rw = Chem.MolFromMolBlock(mol.molfile)
        elif mol.cxsmiles:
            # RDKit parses CXSMILES via MolFromSmiles when CX extensions are present
            rw = Chem.MolFromSmiles(mol.cxsmiles)
        else:
            smiles = structure_smiles(mol)
            if not smiles:
                raise ValueError("rdkit backend requires smiles, cxsmiles, esmiles, or molfile")
            rw = Chem.MolFromSmiles(smiles)
        if rw is None:
            raise ValueError("RDKit failed to parse molecule")

        # Prefer embedded CX coords when present; otherwise compute 2D
        if rw.GetNumConformers() == 0:
            AllChem.Compute2DCoords(rw)
        conf = rw.GetConformer()
        atoms: list[AtomLayout] = []
        for a in rw.GetAtoms():
            i = a.GetIdx()
            p = conf.GetAtomPosition(i)
            el = a.GetSymbol()
            label = None if el == "C" and a.GetFormalCharge() == 0 else el
            atoms.append(
                AtomLayout(
                    index=i,
                    element=el,
                    x=float(p.x),
                    y=float(p.y),
                    charge=a.GetFormalCharge(),
                    label=label,
                )
            )
        bonds: list[BondLayout] = []
        for b in rw.GetBonds():
            bonds.append(
                BondLayout(
                    index=b.GetIdx(),
                    begin=b.GetBeginAtomIdx(),
                    end=b.GetEndAtomIdx(),
                    order=float(b.GetBondTypeAsDouble()),
                )
            )
        return MoleculeLayout(id=mol.id, atoms=atoms, bonds=bonds, backend=self.name)

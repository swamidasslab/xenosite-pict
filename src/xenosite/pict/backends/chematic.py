"""Chematic layout backend (last-resort optional extra)."""

from __future__ import annotations

from xenosite.pict.backends.base import register, warn_unsupported
from xenosite.pict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec
from xenosite.pict.structure import structure_smiles


@register("chematic")
class ChematicBackend:
    name = "chematic"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        try:
            import chematic
        except ImportError as e:
            raise ImportError(
                "Chematic backend requires chematic. "
                "Install with: pip install 'xenosite-pict[chematic]'"
            ) from e

        warn_unsupported(
            self.name,
            "coordinate_quality",
            "Chematic 2D coords are a last resort; prefer indigo, rdkit, or openbabel.",
        )
        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES before <sep> only.")
        if mol.cxsmiles:
            warn_unsupported(
                self.name, "cxsmiles", "Stripping ChemAxon extensions after '|'."
            )

        raw = structure_smiles(mol)
        if not raw:
            raise ValueError("chematic backend requires smiles, cxsmiles, or esmiles")
        smiles = raw.split("|", 1)[0].strip()

        cmol = chematic.from_smiles(smiles)
        data = cmol.depict_data()
        atoms: list[AtomLayout] = []
        for a in data["atoms"]:
            el = a.get("element") or a.get("symbol") or "C"
            label = a.get("label")
            if label is None and el != "C":
                label = el
            atoms.append(
                AtomLayout(
                    index=int(a["idx"]),
                    element=el,
                    x=float(a["x"]),
                    y=float(a["y"]),
                    charge=int(a.get("charge") or 0),
                    label=label,
                )
            )
        bonds: list[BondLayout] = []
        for b in data["bonds"]:
            kind = str(b.get("kind", "Single")).lower()
            order = {"single": 1.0, "double": 2.0, "triple": 3.0, "aromatic": 1.5}.get(
                kind, 1.0
            )
            bonds.append(
                BondLayout(
                    index=int(b["idx"]),
                    begin=int(b["atom1"]),
                    end=int(b["atom2"]),
                    order=order,
                )
            )
        return MoleculeLayout(id=mol.id, atoms=atoms, bonds=bonds, backend=self.name)

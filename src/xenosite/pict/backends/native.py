"""Native pure-Python layout (experimental depictor lab).

Parses a restricted organic SMILES and places rings as regular polygons with
120° chain zig-zag (CDK/RDKit-inspired). Bridged/cage systems are not forced
all-regular. Prefer Indigo for transitional publication coords until the native
quality checklist is green — see ``docs/layout-notes.md``.
"""

from __future__ import annotations

from xenosite.pict.backends.base import register, warn_unsupported
from xenosite.pict.backends.native_layout import layout_smiles
from xenosite.pict.contracts.layout import MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec
from xenosite.pict.structure import structure_smiles


@register("native")
class NativeBackend:
    name = "native"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        if mol.cxsmiles:
            warn_unsupported(
                self.name,
                "cxsmiles",
                "Native strips CXSMILES extensions after '|'.",
            )
        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES portion only if present.")
        if mol.molfile:
            warn_unsupported(self.name, "molfile", "Ignored by native layout.")

        raw = structure_smiles(mol) or ""
        smiles = raw.split("|", 1)[0].strip()
        if not smiles:
            raise ValueError("native backend requires smiles, cxsmiles, or esmiles")
        return layout_smiles(smiles, mol_id=mol.id)

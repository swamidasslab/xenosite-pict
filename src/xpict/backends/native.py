"""Native pure-Python layout (experimental depictor lab).

Parses a restricted organic SMILES and places rings as regular polygons with
120° chain zig-zag (CDK/RDKit-inspired). Bridged/cage systems are not forced
all-regular. Prefer Indigo for transitional publication coords until the native
quality checklist is green — see ``docs/layout-notes.md``.
"""

from __future__ import annotations

from xpict.backends.base import register, warn_unsupported
from xpict.backends.native_layout import layout_smiles
from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.spec import MoleculeSpec
from xpict.structure import cx_atom_labels, structure_smiles


@register("native")
class NativeBackend:
    name = "native"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        raw = structure_smiles(mol) or ""
        if mol.cxsmiles:
            warn_unsupported(
                self.name,
                "cxsmiles",
                "Native uses topology from SMILES; CX atom aliases still applied to labels.",
            )
        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES portion only if present.")
        if mol.molfile:
            warn_unsupported(self.name, "molfile", "Ignored by native layout.")

        smiles = raw.split("|", 1)[0].strip()
        if not smiles:
            raise ValueError("native backend requires smiles, cxsmiles, or esmiles")
        layout = layout_smiles(smiles, mol_id=mol.id)
        aliases = cx_atom_labels(raw)
        if not aliases:
            return layout
        atoms = []
        for a in layout.atoms:
            if a.index < len(aliases) and aliases[a.index]:
                atoms.append(a.model_copy(update={"label": aliases[a.index]}))
            else:
                atoms.append(a)
        return layout.model_copy(update={"atoms": atoms})

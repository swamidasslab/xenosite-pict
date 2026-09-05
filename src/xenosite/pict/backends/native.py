"""Native pure-Python layout stub (last resort / tests without chem engines)."""

from __future__ import annotations

import math
import re

from xenosite.pict.backends.base import register, warn_unsupported
from xenosite.pict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec
from xenosite.pict.structure import structure_smiles

_ATOM = re.compile(r"(\[[^\]]+\]|Br|Cl|Si|Se|B|C|N|O|P|S|F|I|b|c|n|o|p|s|\*)")


def _element_from_token(tok: str) -> str:
    if tok.startswith("["):
        m = re.match(r"\[([A-Za-z]{1,2})", tok)
        return (m.group(1) if m else "C").capitalize()
    return tok[0].upper() + tok[1:] if len(tok) > 1 and tok[1].islower() else tok.upper()


@register("native")
class NativeBackend:
    name = "native"

    def layout(self, mol: MoleculeSpec) -> MoleculeLayout:
        if mol.cxsmiles:
            warn_unsupported(
                self.name,
                "cxsmiles",
                "Native stub strips CXSMILES extensions after '|'.",
            )
        if mol.esmiles:
            warn_unsupported(self.name, "esmiles", "Using SMILES portion only if present.")
        if mol.molfile:
            warn_unsupported(self.name, "molfile", "Ignored by native stub.")

        raw = structure_smiles(mol) or ""
        # Strip ChemAxon extension block if present
        smiles = raw.split("|", 1)[0].strip()
        if not smiles:
            raise ValueError("native backend requires smiles, cxsmiles, or esmiles")

        tokens = _ATOM.findall(smiles)
        if not tokens:
            tokens = ["C"]
        n = len(tokens)
        bond_length = 1.5
        atoms: list[AtomLayout] = []
        for i, tok in enumerate(tokens):
            angle = (i / max(n - 1, 1)) * math.pi * 0.35 - math.pi * 0.175
            x = i * bond_length * math.cos(0.15)
            y = i * bond_length * math.sin(angle) * 0.3
            el = _element_from_token(tok)
            label = None if el == "C" else el
            atoms.append(AtomLayout(index=i, element=el, x=x, y=y, label=label))

        bonds = [
            BondLayout(index=i, begin=i, end=i + 1, order=1.0) for i in range(n - 1)
        ]
        return MoleculeLayout(
            id=mol.id,
            atoms=atoms,
            bonds=bonds,
            backend=self.name,
            warnings=["native backend uses a toy layout; prefer indigo, rdkit, or openbabel"],
        )

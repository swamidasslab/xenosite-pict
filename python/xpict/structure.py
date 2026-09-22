"""Shared helpers for parsing molecule structure strings."""

from __future__ import annotations

import re

from xpict.future.spec import MoleculeSpec

# ChemAxon ``|$alias1;alias2;$|`` atom-label block inside CXSMILES.
_CX_ATOM_LABELS = re.compile(r"\|\$([^|]*)\$\|")


def structure_smiles(mol: MoleculeSpec) -> str | None:
    """Best-effort SMILES-like string (order: smiles, cxsmiles, esmiles)."""
    if mol.smiles:
        return mol.smiles
    if mol.cxsmiles:
        return mol.cxsmiles
    if mol.esmiles:
        return mol.esmiles.split("<sep>")[0]
    return None


def cx_atom_labels(smiles_or_cx: str) -> list[str | None]:
    """Parse CXSMILES ``|$a;b;c;$|`` aliases into a per-atom label list.

    Empty segments become ``None``. Returns ``[]`` when no label block is present.
    Leading underscore in ChemAxon (``_R``) is stripped for display.
    """
    m = _CX_ATOM_LABELS.search(smiles_or_cx)
    if not m:
        return []
    raw = m.group(1)
    # Trailing empty field before final ``$`` is common: ``a;b;``.
    parts = raw.split(";")
    if parts and parts[-1] == "":
        parts = parts[:-1]
    out: list[str | None] = []
    for p in parts:
        p = p.strip()
        if not p:
            out.append(None)
            continue
        if p.startswith("_") and len(p) > 1:
            p = p[1:]
        out.append(p)
    return out

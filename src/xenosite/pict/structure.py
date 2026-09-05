"""Shared helpers for parsing molecule structure strings."""

from __future__ import annotations

from xenosite.pict.contracts.spec import MoleculeSpec


def structure_smiles(mol: MoleculeSpec) -> str | None:
    """Best-effort SMILES-like string (order: smiles, cxsmiles, esmiles)."""
    if mol.smiles:
        return mol.smiles
    if mol.cxsmiles:
        return mol.cxsmiles
    if mol.esmiles:
        return mol.esmiles.split("<sep>")[0]
    return None

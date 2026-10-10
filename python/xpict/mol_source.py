"""SMILES / CXSMILES / molfile → MolNode field dict."""

from __future__ import annotations


def source_to_mol_fields(source: str) -> dict[str, str]:
    text = source.strip()
    if "V2000" in text or "V3000" in text or text.count("\n") >= 3:
        return {"molfile": text}
    if "|" in text:
        return {"cxsmiles": text}
    return {"smiles": text}

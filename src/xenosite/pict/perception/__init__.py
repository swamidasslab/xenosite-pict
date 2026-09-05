"""Chem perception helpers — prefer Chematic when installed.

Perception (parse, aromaticity, SSSR, stereo flags) is separate from depiction.
Chematic is a small Rust RDKit-like kernel for perception; its 2D coords are
weak — do not use them for publication layout. See ``docs/layout-notes.md``.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class PerceivedAtom:
    index: int
    element: str
    charge: int = 0
    aromatic: bool = False


@dataclass
class PerceivedBond:
    index: int
    begin: int
    end: int
    order: float
    aromatic: bool = False
    stereo: str | None = None  # up/down/either/none


@dataclass
class PerceivedMol:
    atoms: list[PerceivedAtom]
    bonds: list[PerceivedBond]
    engine: str
    rings: list[tuple[int, ...]] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


def chematic_available() -> bool:
    try:
        import chematic  # noqa: F401

        return True
    except ImportError:
        return False


def perceive_smiles(smiles: str) -> PerceivedMol:
    """Perceive a SMILES string via Chematic when available.

    Raises ImportError if Chematic is not installed. Callers that need a
    guaranteed path should catch and fall back to RDKit/Indigo layout backends.
    """
    try:
        import chematic
    except ImportError as e:
        raise ImportError(
            "Chematic perception requires chematic. "
            "Install with: pip install 'xenosite-pict[chematic]'"
        ) from e

    raw = smiles.split("|", 1)[0].strip()
    mol = chematic.from_smiles(raw)
    warnings: list[str] = [
        "chematic used for perception only; do not use chematic coords for depiction"
    ]

    atoms: list[PerceivedAtom] = []
    bonds: list[PerceivedBond] = []
    rings: list[tuple[int, ...]] = []

    # Prefer structured APIs when present; fall back to depict_data topology.
    if hasattr(mol, "atoms") and callable(getattr(mol, "atoms", None)):
        for i, a in enumerate(mol.atoms()):
            el = getattr(a, "symbol", None) or getattr(a, "element", lambda: "C")()
            if callable(el):
                el = el()
            charge = int(getattr(a, "charge", lambda: 0)() if callable(getattr(a, "charge", None)) else getattr(a, "charge", 0) or 0)
            aromatic = bool(getattr(a, "is_aromatic", lambda: False)() if callable(getattr(a, "is_aromatic", None)) else False)
            atoms.append(PerceivedAtom(index=i, element=str(el), charge=charge, aromatic=aromatic))
    else:
        data: dict[str, Any] = mol.depict_data() if hasattr(mol, "depict_data") else {}
        for a in data.get("atoms", []):
            el = a.get("element") or a.get("symbol") or "C"
            atoms.append(
                PerceivedAtom(
                    index=int(a["idx"]),
                    element=str(el),
                    charge=int(a.get("charge") or 0),
                    aromatic=bool(a.get("aromatic") or False),
                )
            )
        for b in data.get("bonds", []):
            kind = str(b.get("kind", "Single")).lower()
            order = {"single": 1.0, "double": 2.0, "triple": 3.0, "aromatic": 1.5}.get(kind, 1.0)
            bonds.append(
                PerceivedBond(
                    index=int(b["idx"]),
                    begin=int(b["atom1"]),
                    end=int(b["atom2"]),
                    order=order,
                    aromatic=kind == "aromatic",
                )
            )

    if not bonds and hasattr(mol, "bonds"):
        for i, b in enumerate(mol.bonds()):
            begin = getattr(b, "begin", None) or getattr(b, "atom1", None)
            end = getattr(b, "end", None) or getattr(b, "atom2", None)
            if callable(begin):
                begin = begin()
            if callable(end):
                end = end()
            order = getattr(b, "order", None) or getattr(b, "bond_order", lambda: 1)()
            if callable(order):
                order = order()
            aromatic = bool(getattr(b, "is_aromatic", lambda: False)() if callable(getattr(b, "is_aromatic", None)) else False)
            bonds.append(
                PerceivedBond(
                    index=i,
                    begin=int(begin),
                    end=int(end),
                    order=float(order),
                    aromatic=aromatic,
                )
            )

    # SSSR if Chematic exposes it
    for attr in ("sssr", "rings", "get_sssr"):
        if hasattr(mol, attr):
            val = getattr(mol, attr)
            val = val() if callable(val) else val
            try:
                rings = [tuple(int(x) for x in r) for r in val]
            except Exception:
                rings = []
            break

    return PerceivedMol(atoms=atoms, bonds=bonds, engine="chematic", rings=rings, warnings=warnings)

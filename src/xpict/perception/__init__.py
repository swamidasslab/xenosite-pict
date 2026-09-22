"""Chem perception helpers (SSSR / aromaticity / stereo flags).

Chematic is **not** used. Perception belongs in native / Rust over time.
This module keeps light dataclasses for tests and future hooks.
"""

from __future__ import annotations

from dataclasses import dataclass, field


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


__all__ = [
    "PerceivedAtom",
    "PerceivedBond",
    "PerceivedMol",
]

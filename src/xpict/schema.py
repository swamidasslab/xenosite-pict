"""Pydantic document models (plan name ``schema.py``).

Canonical definitions live in ``xpict.contracts.spec``; this module
re-exports them for the scaffold layout described in the project plan.
"""

from __future__ import annotations

from xpict.contracts.spec import (
    DiagramKind,
    DiagramSpec,
    EdgeArrow,
    EdgeSpec,
    MarkKind,
    MarkSpec,
    MoleculeSpec,
    PictSpec,
    ShadeSpec,
)

__all__ = [
    "DiagramKind",
    "DiagramSpec",
    "EdgeArrow",
    "EdgeSpec",
    "MarkKind",
    "MarkSpec",
    "MoleculeSpec",
    "PictSpec",
    "ShadeSpec",
]

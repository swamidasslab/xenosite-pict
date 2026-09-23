"""xpict — molecule depiction.

**Declarative document:** nested ``DepictSpec`` (``type: "mol"`` /
``type: "group"`` + ``children``), a strict subset of future ``PictSpec`` that
is still expanding.

**Single molecule (JS/Rust):** ``mol`` / ``render`` / ``to_svg`` for one-mol
callers. That Mol-object client is not on PyPI yet — use ``render(doc)`` here.
"""

from __future__ import annotations

from xpict.api import Pict, render
from xpict.contracts.depict import DepictSpec, MolSpec
from xpict.contracts.scene import Scene
from xpict.warnings import PictBackendWarning

# Lab / future nested document.
from xpict.future import PictSpec

__version__ = "0.1.4"

__all__ = [
    "DepictSpec",
    "MolSpec",
    "Pict",
    "PictBackendWarning",
    "PictSpec",  # future nested document — use DepictSpec for the shipped subset
    "Scene",
    "render",
    "__version__",
]

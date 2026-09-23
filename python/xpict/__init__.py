"""xpict — molecule depiction.

**Preferred:** nested document (``DepictSpec`` — ``type: "mol"`` /
``type: "group"`` + ``children``), a strict subset of future ``PictSpec``.

**Simple (JS/Rust today):** ``mol`` / ``render`` / ``to_svg`` for single-mol
callers; the document path uses that layer internally.
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
    "PictSpec",  # future — prefer MolSpec / DepictSpec for new code
    "Scene",
    "render",
    "__version__",
]

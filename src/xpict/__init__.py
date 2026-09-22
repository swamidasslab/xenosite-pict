"""xpict — molecule depiction (public: mol / render / depict / to_svg)."""

from __future__ import annotations

from xpict.api import Pict, render
from xpict.contracts.depict import DepictSpec, MolSpec
from xpict.contracts.scene import Scene
from xpict.warnings import PictBackendWarning

# Lab / future nested document (not the ship surface).
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

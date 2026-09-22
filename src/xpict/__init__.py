"""xpict — declarative molecule depiction."""

from __future__ import annotations

from xpict.api import Pict, render
from xpict.client import (
    Mol,
    MolRenderOptions,
    Rendered,
    Xpict,
    mol,
    render as render_mol,
    to_svg,
    xpict,
)
from xpict.contracts.nodes import PictSpec
from xpict.warnings import PictBackendWarning

__version__ = "0.1.4"

__all__ = [
    "Mol",
    "MolRenderOptions",
    "Pict",
    "PictBackendWarning",
    "PictSpec",
    "Rendered",
    "Xpict",
    "mol",
    "render",
    "render_mol",
    "to_svg",
    "xpict",
    "__version__",
]

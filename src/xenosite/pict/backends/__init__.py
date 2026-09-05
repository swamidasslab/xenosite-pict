"""Layout backends.

Preference: indigo → rdkit → openbabel → chematic → native.
"""

from __future__ import annotations

from xenosite.pict.backends import chematic as _chematic  # noqa: F401
from xenosite.pict.backends import indigo as _indigo  # noqa: F401
from xenosite.pict.backends import native as _native  # noqa: F401
from xenosite.pict.backends import openbabel as _openbabel  # noqa: F401
from xenosite.pict.backends import rdkit as _rdkit  # noqa: F401
from xenosite.pict.backends.base import LayoutBackend, get_backend

BACKEND_PREFERENCE = ("indigo", "rdkit", "openbabel", "chematic", "native")

__all__ = ["BACKEND_PREFERENCE", "LayoutBackend", "get_backend"]

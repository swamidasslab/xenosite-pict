"""Layout backends.

Transitional coords: **Indigo** only. Goal: **native** depictor proven on the
hard-case gallery — then Indigo shrinks to optional parse/emergency fallback.
No RDKit / Open Babel / Chematic layout backends; Chematic stays perception-only.
"""

from __future__ import annotations

from xenosite.pict.backends import indigo as _indigo  # noqa: F401
from xenosite.pict.backends import native as _native  # noqa: F401
from xenosite.pict.backends.base import LayoutBackend, get_backend

BACKEND_PREFERENCE = ("indigo", "native")

__all__ = ["BACKEND_PREFERENCE", "LayoutBackend", "get_backend"]

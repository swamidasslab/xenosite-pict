"""Layout backends.

**native** is the default / product path. **Indigo** is an optional alternate
layout backend (``xpict[indigo]``, ``backend="indigo"``).
RDKit is optional and used only for template alignment (``align.select_aligner``),
called from Python — not a layout backend. Chematic is not used.
"""

from __future__ import annotations

from xpict.backends import indigo as _indigo  # noqa: F401
from xpict.backends import native as _native  # noqa: F401
from xpict.backends.base import LayoutBackend, get_backend

# Preference when resolving an explicit unset? Default path uses native first.
BACKEND_PREFERENCE = ("native", "indigo")

__all__ = ["BACKEND_PREFERENCE", "LayoutBackend", "get_backend"]

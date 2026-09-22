"""Layout backends.

**Focus:** **RDKit** for 2D coordinates and template alignment (Python ``rdkit``;
JS ``@rdkit/rdkit`` — never linked into Rust).

| Backend | Role |
| --- | --- |
| ``rdkit`` | Preferred layout when installed (``xpict[rdkit]``) |
| ``native`` | In-house layout (long-term product path; still maturing) |
| ``indigo`` | Alternate layout; use **rigid / fake align** in Rust (no RDKit template) |

Chematic is not used.
"""

from __future__ import annotations

from xpict.backends import indigo as _indigo  # noqa: F401
from xpict.backends import native as _native  # noqa: F401
from xpict.backends import rdkit_layout as _rdkit  # noqa: F401
from xpict.backends.base import LayoutBackend, get_backend

# Resolve default: RDKit if importable, else native. Indigo is opt-in.
BACKEND_PREFERENCE = ("rdkit", "native", "indigo")

__all__ = ["BACKEND_PREFERENCE", "LayoutBackend", "get_backend"]

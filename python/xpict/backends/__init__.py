"""Layout backends.

**Focus:** **RDKit** for 2D coordinates and template alignment (Python ``rdkit``;
JS ``@rdkit/rdkit`` — never linked into Rust).

| Backend | Role |
| --- | --- |
| ``rdkit`` | Default layout when installed (``xpict[rdkit]``) |
| ``indigo`` | Alternate layout; use **rigid / fake align** in Rust (no RDKit template) |

Chematic is not used. There is no in-house SMILES layout backend.
"""

from __future__ import annotations

from xpict.backends import indigo as _indigo
from xpict.backends import rdkit_layout as _rdkit
from xpict.backends.base import LayoutBackend, get_backend

# Side-effect imports register backends; keep references so imports are used.
_REGISTERED = (_indigo, _rdkit)

# Resolve default: RDKit when importable. Indigo is opt-in only.
BACKEND_PREFERENCE = ("rdkit", "indigo")

__all__ = ["BACKEND_PREFERENCE", "LayoutBackend", "get_backend"]

"""RDKit 2D coordinate defaults for xpict language edges.

Prefer Schrödinger CoordGen for **free** ``Compute2DCoords`` when the
installed RDKit build includes it (``IsCoordGenSupportAvailable``).
Falls back to RDKit's native Depictor otherwise.

Constrained alignment tries CoordGen first (bond-length parity with free
layout), then ``forceRDKit`` if the core fails to lock. MinimalLib align
keeps ``useCoordGen:false`` — WASM CoordGen align does not lock cores.
"""

from __future__ import annotations

_enabled: bool | None = None


def prefer_coordgen() -> bool:
    """Enable CoordGen as the process default when available.

    Returns whether CoordGen is active after the call.
    """
    global _enabled
    if _enabled is not None:
        return _enabled
    try:
        from rdkit.Chem import rdDepictor
    except ImportError:
        _enabled = False
        return False
    if rdDepictor.IsCoordGenSupportAvailable():
        rdDepictor.SetPreferCoordGen(True)
        _enabled = True
    else:
        _enabled = False
    return _enabled

"""Multi-molecule chemical alignment (stub).

Future: port xenopict MCS spanning-tree alignment onto backend-agnostic
layout graphs so related molecules share a common frame before diagram
placement (ELK / grid).

Today ``PictSpec.align`` is accepted but not applied — diagram layout only.
"""

from __future__ import annotations

from collections.abc import Sequence

from xenosite.pict.contracts.layout import MoleculeLayout
from xenosite.pict.warnings import PictBackendWarning


def align_layouts(
    layouts: Sequence[MoleculeLayout],
    *,
    enabled: bool = False,
) -> list[MoleculeLayout]:
    """Return layouts unchanged until MCS alignment is implemented."""
    if enabled and len(layouts) > 1:
        import warnings

        warnings.warn(
            "PictSpec.align=True is accepted but MCS alignment is not implemented yet",
            PictBackendWarning,
            stacklevel=2,
        )
    return list(layouts)

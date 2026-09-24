"""Live package contracts — implemented public surface only.

Full nested ``PictSpec`` / shorthand / diagram chrome lives under
``xpict.future`` for design review until those features ship.
"""

from __future__ import annotations

from xpict.contracts.depict import AlignToSpec, GroupNode, MolNode
from xpict.contracts.depict import DepictSpec as _DepictSpec
from xpict.contracts.edge import (
    AlignOpts,
    AtomIn,
    BondIn,
    CoordGenMoleculeResult,
    CoordGenTask,
    CoordGenTaskResult,
    CoordMethod,
    EdgePlan,
    EdgeResult,
    MoleculeIn,
    MolTemplate,
)
from xpict.contracts.layout import (
    AtomLayout,
    BondLayout,
    MoleculeLayout,
)
from xpict.contracts.scene import (
    CirclePrim,
    Layer,
    PathPrim,
    Primitive,
    Scene,
    TextPrim,
    Viewport,
)

# Public alias (Rust ``type MolSpec = MolNode``); not on the wire schema.
MolSpec = MolNode


class DepictSpec(_DepictSpec):
    """Live document root — generated wire model + host ``mols()`` helper."""

    def mols(self) -> list[MolNode]:
        """Flatten mol root or group children (mirrors Rust ``DepictSpec::mols``)."""
        root = self.root
        if isinstance(root, MolNode):
            return [root]
        return list(root.children)


__all__ = [
    "AlignOpts",
    "AlignToSpec",
    "AtomIn",
    "AtomLayout",
    "BondIn",
    "BondLayout",
    "CirclePrim",
    "CoordGenMoleculeResult",
    "CoordGenTask",
    "CoordGenTaskResult",
    "CoordMethod",
    "DepictSpec",
    "EdgePlan",
    "EdgeResult",
    "GroupNode",
    "Layer",
    "MolNode",
    "MolSpec",
    "MolTemplate",
    "MoleculeIn",
    "MoleculeLayout",
    "PathPrim",
    "Primitive",
    "Scene",
    "TextPrim",
    "Viewport",
]

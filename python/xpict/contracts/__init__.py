"""Live package contracts — implemented public surface only.

Full nested ``PictSpec`` / shorthand / diagram chrome lives under
``xpict.future`` for design review until those features ship.
"""

from __future__ import annotations

from xpict.contracts.depict import (
    AlignToSpec,
    EdgeNode,
    GroupNode,
    Label,
    LabelLanes,
    LabelPlacement,
    LayoutOpts,
    MolNode,
    ReactionSchemeNode,
    TextNode,
)
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
        """Flatten mol nodes in document order (skips edge children)."""
        root = self.root
        if isinstance(root, MolNode):
            return [root]
        if isinstance(root, ReactionSchemeNode):
            return [c for c in root.children if isinstance(c, MolNode)]
        return [c for c in root.children if isinstance(c, MolNode)]


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
    "EdgeNode",
    "EdgePlan",
    "EdgeResult",
    "GroupNode",
    "Label",
    "LabelLanes",
    "LabelPlacement",
    "Layer",
    "LayoutOpts",
    "MolNode",
    "MolSpec",
    "MolTemplate",
    "MoleculeIn",
    "MoleculeLayout",
    "PathPrim",
    "Primitive",
    "ReactionSchemeNode",
    "Scene",
    "TextNode",
    "TextPrim",
    "Viewport",
]

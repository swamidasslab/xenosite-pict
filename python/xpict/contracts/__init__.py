"""Live package contracts — implemented public surface only.

Full nested ``PictSpec`` / shorthand / diagram chrome lives under
``xpict.future`` for design review until those features ship.
"""

from xpict.contracts.depict import AlignToSpec, DepictSpec, GroupNode, MolNode, MolSpec
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

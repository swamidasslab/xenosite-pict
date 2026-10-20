"""Live package contracts — generated wire types (pydantic + TypedDict).

Host ``DepictSpec`` (operators / ``PictBase``) lives in ``xpict.spec``.
"""

from __future__ import annotations

from xpict.contracts.depict import (
    AlignToSpec,
    EdgeNode,
    ForTypesPatch,
    GridNode,
    GroupNode,
    Label,
    LabelLanes,
    LabelPlacement,
    LayoutOpts,
    MolNode,
    MolOpts,
    MolOptsPatch,
    Opts,
    OptsPatch,
    ReactionSchemeNode,
    TextNode,
)
from xpict.contracts.depict_dict import DepictSpecDict, OptsPatchDict
from xpict.contracts.edge import (
    AlignMode,
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
from xpict.contracts.edge_dict import EdgePlanDict, EdgeResultDict
from xpict.contracts.rendered_dict import DocPaintDict, RenderedDict
from xpict.contracts.scene import (
    CirclePrim,
    Layer,
    PathPrim,
    Primitive,
    Scene,
    TextPrim,
    Viewport,
)
from xpict.contracts.scene_dict import SceneDict

# Public alias (Rust ``type MolSpec = MolNode``); not on the wire schema.
MolSpec = MolNode


__all__ = [
    "AlignMode",
    "AlignOpts",
    "AlignToSpec",
    "AtomIn",
    "BondIn",
    "CirclePrim",
    "CoordGenMoleculeResult",
    "CoordGenTask",
    "CoordGenTaskResult",
    "CoordMethod",
    "DepictSpecDict",
    "DocPaintDict",
    "EdgeNode",
    "EdgePlan",
    "EdgePlanDict",
    "EdgeResult",
    "EdgeResultDict",
    "ForTypesPatch",
    "GridNode",
    "GroupNode",
    "Label",
    "LabelLanes",
    "LabelPlacement",
    "Layer",
    "LayoutOpts",
    "MolNode",
    "MolOpts",
    "MolOptsPatch",
    "MolSpec",
    "MolTemplate",
    "MoleculeIn",
    "Opts",
    "OptsPatch",
    "OptsPatchDict",
    "PathPrim",
    "Primitive",
    "ReactionSchemeNode",
    "RenderedDict",
    "Scene",
    "SceneDict",
    "TextNode",
    "TextPrim",
    "Viewport",
]

# Auto-generated from Rust schemars (make types) — do not edit.
"""DepictSpec — live document ABI from xpict-core (schemars). TypedDict wire forms (no ``dict[str, Any]``)."""

from __future__ import annotations

from typing import Mapping, Literal, NotRequired, Required, TypedDict

AlignMode = Literal['exact', 'greedy', 'skip']


class AlignToSpecDict(TypedDict, total=False):
    """Object form of document ``align_to`` (template ref + EdgePlan-style opts)."""
    ref: Required[str]
    atom_map: NotRequired[list[tuple[int, int]]]
    min_atoms: NotRequired[int]
    mode: NotRequired[AlignMode]
    timeout_ms: NotRequired[int]


AlignToDict = str | AlignToSpecDict


AromaticStyle = Literal['circle', 'kekule']


RegionTextPlace = Literal['outside', 'inside']


LabelPos = Literal['above', 'below', 'left', 'right']


class MolTextSpecDict(TypedDict, total=False):
    """Expanded text content for mol name, region labels, callouts, atom labels."""
    color: NotRequired[str]
    content: NotRequired[str]
    margin: NotRequired[float]
    place: NotRequired[RegionTextPlace]
    pos: NotRequired[LabelPos]
    scale: NotRequired[float]


MolTextDict = str | MolTextSpecDict


class AtomLabelDetailedDict(TypedDict, total=False):
    """Expanded atom-label bag with optional shared scale/color/z."""
    color: NotRequired[str]
    labels: NotRequired[Mapping[str, MolTextDict]]
    scale: NotRequired[float]
    z: NotRequired[int]


AtomLabelSpecDict = Mapping[str, MolTextDict] | AtomLabelDetailedDict


AtomNumberBy = Literal['index', 'mapno']


class AtomNumberPolicyDict(TypedDict, total=False):
    """Expanded atom-number policy."""
    by: NotRequired[AtomNumberBy]
    color: NotRequired[str]
    only: NotRequired[list[int]]
    scale: NotRequired[float]
    z: NotRequired[int]


AtomNumberSpecDict = AtomNumberBy | AtomNumberPolicyDict


class CalloutSpecDict(TypedDict, total=False):
    """Callout annotation (arrow / freer placement)."""
    text: Required[MolTextDict]
    arrow: NotRequired[bool]
    atoms: NotRequired[list[int]]
    bonds: NotRequired[list[tuple[int, int]]]
    color: NotRequired[str]
    prefer: NotRequired[LabelPos]
    scale: NotRequired[float]
    z: NotRequired[int]


CalloutFieldDict = CalloutSpecDict | list[CalloutSpecDict]


class FillSpecDict(TypedDict, total=False):
    """Fill ink for region / circle."""
    color: NotRequired[str]
    opacity: NotRequired[float]


class StrokeSpecDict(TypedDict, total=False):
    """Stroke ink for region / circle."""
    color: NotRequired[str]
    dash: NotRequired[str]
    width: NotRequired[float]


class CircleSpecDict(TypedDict, total=False):
    """Atom or ring highlight circle."""
    atoms: NotRequired[list[int]]
    fill: NotRequired[FillSpecDict]
    ring: NotRequired[list[int]]
    scale: NotRequired[float]
    stroke: NotRequired[StrokeSpecDict]
    z: NotRequired[int]


CircleFieldDict = CircleSpecDict | list[CircleSpecDict]


class LabelPlacementDict(TypedDict, total=False):
    """Placed label: `{ "id": "adh", "pos": "below" }` (`pos` optional → above)."""
    id: Required[str]
    pos: NotRequired[LabelPos]


LabelItemDict = str | LabelPlacementDict


class LabelLanesDict(TypedDict, total=False):
    """Lane bag: `{ "above": [...], "below": [...], "left": [...], "right": [...] }`."""
    above: NotRequired[list[str]]
    below: NotRequired[list[str]]
    left: NotRequired[list[str]]
    right: NotRequired[list[str]]


LabelDict = str | list[LabelItemDict] | LabelPlacementDict | LabelLanesDict


class ShadeStyleDict(TypedDict, total=False):
    """Cascading shade window / LUT (not per-atom scores)."""
    colormap: NotRequired[str]
    vmax: NotRequired[float]
    vmin: NotRequired[float]


class MolOptsPatchDict(TypedDict, total=False):
    """Mol cascading paint opts — what a mol leaf consumes from the cascade."""
    type: Required[Literal['mol']]
    aromatic: NotRequired[AromaticStyle]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    halo: NotRequired[bool]
    pad: NotRequired[float]
    scale: NotRequired[float]
    shade: NotRequired[ShadeStyleDict]
    weight: NotRequired[float]


class GroupOptsPatchDict(TypedDict, total=False):
    """Universal cascading keys (any node kind)."""
    type: Required[Literal['group']]
    aromatic: NotRequired[AromaticStyle]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    pad: NotRequired[float]
    scale: NotRequired[float]


class ReactionSchemeOptsPatchDict(TypedDict, total=False):
    """Universal cascading keys (any node kind)."""
    type: Required[Literal['reaction_scheme']]
    aromatic: NotRequired[AromaticStyle]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    pad: NotRequired[float]
    scale: NotRequired[float]


class EdgeOptsPatchDict(TypedDict, total=False):
    """Universal cascading keys (any node kind)."""
    type: Required[Literal['edge']]
    aromatic: NotRequired[AromaticStyle]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    pad: NotRequired[float]
    scale: NotRequired[float]


class TextOptsPatchDict(TypedDict, total=False):
    """Universal cascading keys (any node kind)."""
    type: Required[Literal['text']]
    aromatic: NotRequired[AromaticStyle]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    pad: NotRequired[float]
    scale: NotRequired[float]


TypedOptsPatchDict = MolOptsPatchDict | GroupOptsPatchDict | ReactionSchemeOptsPatchDict | EdgeOptsPatchDict | TextOptsPatchDict


NodeType = Literal['mol', 'group', 'reaction_scheme', 'edge', 'text']


class ForTypesPatchDict(TypedDict, total=False):
    """Multi-kind patch: `{ "for_types": ["mol","group"], …mol opts }`."""
    for_types: Required[list[NodeType]]
    aromatic: NotRequired[AromaticStyle]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    halo: NotRequired[bool]
    pad: NotRequired[float]
    scale: NotRequired[float]
    shade: NotRequired[ShadeStyleDict]
    weight: NotRequired[float]


class MolOptsDict(TypedDict, total=False):
    """Mol cascading paint opts — what a mol leaf consumes from the cascade."""
    aromatic: NotRequired[AromaticStyle]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    halo: NotRequired[bool]
    pad: NotRequired[float]
    scale: NotRequired[float]
    shade: NotRequired[ShadeStyleDict]
    weight: NotRequired[float]


OptsPatchDict = TypedOptsPatchDict | ForTypesPatchDict | MolOptsDict


OptsDict = OptsPatchDict | list[OptsPatchDict]


RegionStyle = Literal['box', 'bezier', 'circle']


class RegionSpecDict(TypedDict, total=False):
    """One region highlight on a mol."""
    atoms: NotRequired[list[int]]
    exclude: NotRequired[list[int]]
    fill: NotRequired[FillSpecDict]
    stroke: NotRequired[StrokeSpecDict]
    style: NotRequired[RegionStyle]
    text: NotRequired[MolTextDict]
    z: NotRequired[int]


RegionFieldDict = RegionSpecDict | list[RegionSpecDict]


class ShadeSpecDict(TypedDict, total=False):
    """Per-atom / per-bond colormap scores (+ legacy window fields on the mol)."""
    atoms: NotRequired[list[float]]
    bonds: NotRequired[list[float]]
    colormap: NotRequired[str]
    vmax: NotRequired[float]
    vmin: NotRequired[float]


MolNodeKind = Literal['mol']


class MolNodeDict(TypedDict, total=False):
    """Mol node — [`MolOpts`] fields + non-cascading identity / scores / align."""
    type: Required[Literal['mol']]
    align_to: NotRequired[AlignToDict]
    aromatic: NotRequired[AromaticStyle]
    atom_label: NotRequired[AtomLabelSpecDict]
    atom_number: NotRequired[AtomNumberSpecDict]
    callout: NotRequired[CalloutFieldDict]
    circle: NotRequired[CircleFieldDict]
    color: NotRequired[str]
    cxsmiles: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    halo: NotRequired[bool]
    id: NotRequired[str]
    label: NotRequired[LabelDict]
    molfile: NotRequired[str]
    name: NotRequired[MolTextDict]
    opts: NotRequired[OptsDict]
    pad: NotRequired[float]
    region: NotRequired[RegionFieldDict]
    scale: NotRequired[float]
    shade: NotRequired[ShadeSpecDict]
    smiles: NotRequired[str]
    star_labels: NotRequired[list[str | None]]
    weight: NotRequired[float]


EdgeArrow = Literal['forward', 'equilibrium', 'open', 'line']


EdgeRouting = Literal['orthogonal', 'polyline', 'splines']


MolIdsDict = str | list[str]


EdgeNodeKind = Literal['edge']


class EdgeNodeDict(TypedDict, total=False):
    """Edge **node** — a reaction / network link between mol ids."""
    type: Required[Literal['edge']]
    sources: Required[MolIdsDict]
    targets: Required[MolIdsDict]
    arrow: NotRequired[EdgeArrow]
    color: NotRequired[str]
    dashed: NotRequired[bool]
    edge_routing: NotRequired[EdgeRouting]
    label: NotRequired[LabelDict]
    role: NotRequired[str]
    stroke_width: NotRequired[float]


TextNodeKind = Literal['text']


class TextNodeDict(TypedDict, total=False):
    """Text **node** — caption / chrome referenced by mols and edges via id."""
    type: Required[Literal['text']]
    text: Required[str]
    color: NotRequired[str]
    id: NotRequired[str]
    opts: NotRequired[OptsDict]
    scale: NotRequired[float]


NodeDict = MolNodeDict | EdgeNodeDict | TextNodeDict


class GroupNodeDict(TypedDict, total=False):
    type: Required[Literal['group']]
    align: NotRequired[bool]
    aromatic: NotRequired[AromaticStyle]
    children: NotRequired[list[NodeDict]]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    id: NotRequired[str]
    opts: NotRequired[OptsDict]
    pad: NotRequired[float]
    scale: NotRequired[float]


LayoutAlgorithm = Literal['layered', 'radial', 'force', 'stress']


LayoutDirection = Literal['right', 'left', 'up', 'down']


class LayoutOptsDict(TypedDict, total=False):
    """Layout for [`DepictSpec::ReactionScheme`]."""
    algorithm: NotRequired[LayoutAlgorithm]
    direction: NotRequired[LayoutDirection]
    edge_routing: NotRequired[EdgeRouting]
    layer_spacing: NotRequired[float]
    node_spacing: NotRequired[float]


class ReactionSchemeNodeDict(TypedDict, total=False):
    """Reaction / pathway scheme: mixed **node** children (`mol` | `edge` | `text`)."""
    type: Required[Literal['reaction_scheme']]
    aromatic: NotRequired[AromaticStyle]
    children: NotRequired[list[NodeDict]]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    id: NotRequired[str]
    layout: NotRequired[LayoutOptsDict]
    opts: NotRequired[OptsDict]
    pad: NotRequired[float]
    scale: NotRequired[float]


class GridNodeDict(TypedDict, total=False):
    """Grid root — each child is one row (`mol` / `group` / `reaction_scheme` / nested `grid`)."""
    type: Required[Literal['grid']]
    aromatic: NotRequired[AromaticStyle]
    children: NotRequired[list[DepictSpecDict]]
    color: NotRequired[str]
    debug_align: NotRequired[bool]
    debug_box: NotRequired[bool]
    id: NotRequired[str]
    opts: NotRequired[OptsDict]
    pad: NotRequired[float]
    scale: NotRequired[float]


DepictSpecDict = MolNodeDict | GroupNodeDict | ReactionSchemeNodeDict | GridNodeDict

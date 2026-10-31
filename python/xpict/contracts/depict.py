# Auto-generated from Rust schemars (make types) — do not edit.
"""DepictSpec — live document ABI from xpict-core (schemars)."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, RootModel, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump_json(*args, **kwargs)

AlignMode = Literal['exact', 'greedy', 'skip']


class AlignToSpec(StrictModel):
    """Object form of document ``align_to`` (template ref + EdgePlan-style opts)."""
    ref: Annotated[str, Field(description='Id of the template mol in this group.')]
    atom_map: Annotated[list[tuple[int, int]] | None, Field(description='Pairs `(query, template)` vs the template; skips MCS when set.')] = None
    min_atoms: Annotated[int | None, Field(description='Override [`crate::edge::MIN_MCS_ATOMS`] when set.')] = None
    mode: Annotated[AlignMode | None, Field(description='`exact` (default) / `greedy` / `skip`. Omit = exact with house timeout.')] = None
    timeout_ms: Annotated[int | None, Field(description='MCS budget in milliseconds. `None` = house 100 ms; `0` = unbounded.')] = None


AlignTo = str | AlignToSpec


AromaticStyle = Literal['circle', 'kekule']


RegionTextPlace = Literal['outside', 'inside']


LabelPos = Literal['above', 'below', 'left', 'right', 'above_left', 'above_right', 'below_left', 'below_right', 'auto']


class MolTextSpec(StrictModel):
    """Expanded text content for mol name, region labels, callouts, atom labels."""
    color: Annotated[str | None, Field(description='CSS color.')] = None
    content: str = Field(default='', description='Display string — chem label markup (`$R_1$`, `N^+`, `**bold**`, …).')
    margin: Annotated[float | None, Field(description='Gap (px) between mol ink AABB and this caption (snug, no clash). Omit → house default (see caption paint `CAPTION_MARGIN_DEFAULT`).')] = None
    place: Annotated[RegionTextPlace | None, Field(description='Region label: inside the hull or outside (default outside).')] = None
    pos: Annotated[LabelPos | None, Field(description='Side relative to the anchor (mol name / outside region label).')] = None
    scale: Annotated[float | None, Field(description='Font scale relative to house atom-label size (mol `name` defaults to 1.5×).')] = None


MolText = str | MolTextSpec


class AtomLabelDetailed(StrictModel):
    """Expanded atom-label bag with optional shared scale/color/z."""
    color: str | None = None
    labels: dict[str, MolText] = Field(default_factory=dict)
    scale: float | None = None
    z: int | None = None


AtomLabelSpec = dict[str, MolText] | AtomLabelDetailed


AtomNumberBy = Literal['index', 'mapno']


class AtomNumberPolicy(StrictModel):
    """Expanded atom-number policy."""
    by: AtomNumberBy = 'index'
    color: str | None = None
    only: Annotated[list[int] | None, Field(description='Restrict to these atom indices (omit = all).')] = None
    scale: float | None = None
    z: int | None = None


AtomNumberSpec = AtomNumberBy | AtomNumberPolicy


class CalloutSpec(StrictModel):
    """Callout annotation (arrow / freer placement)."""
    text: MolText
    arrow: bool = True
    atoms: list[int] | None = None
    bonds: list[tuple[int, int]] | None = None
    color: str | None = None
    prefer: Annotated[LabelPos | None, Field(description='Side bias. Omit or `"auto"` → seed farthest from other ink over the eight compass directions (cardinals + 45°), not a hardcoded above. Locked `"above_right"` / `"below_left"` / … pin that 45° slot.')] = None
    scale: float | None = None
    z: int | None = None


CalloutField = CalloutSpec | list[CalloutSpec]


class FillSpec(StrictModel):
    """Fill ink for region / circle."""
    color: str | None = None
    opacity: float | None = None


class StrokeSpec(StrictModel):
    """Stroke ink for region / circle."""
    color: Annotated[str | None, Field(description='CSS color, or `"none"` to disable stroke.')] = None
    dash: Annotated[str | None, Field(description='`solid` | `dashed` | `dotted` or a raw SVG dasharray.')] = None
    width: float | None = None


class CircleSpec(StrictModel):
    """Atom, bond, or ring highlight."""
    atoms: list[int] = Field(default_factory=list, description='One circle centered on each atom index.')
    bonds: list[tuple[int, int]] = Field(default_factory=list, description='Bond endpoint pairs — one circle at each bond midpoint (`[begin, end]`, order-insensitive).')
    fill: FillSpec | None = None
    ring: Annotated[list[int] | None, Field(description='Ordered ring atoms — one circle at the ring centroid (aromatic-style).')] = None
    scale: Annotated[float | None, Field(description='Radius scale vs house default.')] = None
    stroke: StrokeSpec | None = None
    z: int | None = None


CircleField = CircleSpec | list[CircleSpec]


class LabelPlacement(StrictModel):
    """Placed label: `{ "id": "adh", "pos": "below" }` (`pos` optional → above)."""
    id: Annotated[str, Field(description='Id of a [`TextNode`] or (on edges) [`MolNode`].')]
    pos: LabelPos | None = None


LabelItem = str | LabelPlacement


class LabelLanes(StrictModel):
    """Lane bag: `{ "above": [...], "below": [...], "left": [...], "right": [...] }`."""
    above: list[str] = Field(default_factory=list)
    below: list[str] = Field(default_factory=list)
    left: list[str] = Field(default_factory=list)
    right: list[str] = Field(default_factory=list)


Label = str | list[LabelItem] | LabelPlacement | LabelLanes


class ShadeStyle(StrictModel):
    """Cascading shade window / LUT (not per-atom scores)."""
    colormap: Annotated[str | None, Field(description='Named colormap. Only ``"xenosite"`` (default) is painted; any other name is rejected at depict time (not silently ignored).')] = None
    vmax: float | None = None
    vmin: float | None = None


class MolOptsPatch(StrictModel):
    """Mol cascading paint opts — what a mol leaf consumes from the cascade."""
    type: Literal['mol'] = 'mol'
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    halo: bool | None = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    scale: float | None = None
    shade: ShadeStyle | None = None
    weight: float | None = None


class GroupOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['group'] = 'group'
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    scale: float | None = None


class ReactionSchemeOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['reaction_scheme'] = 'reaction_scheme'
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    scale: float | None = None


class EdgeOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['edge'] = 'edge'
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    scale: float | None = None


class TextOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['text'] = 'text'
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    scale: float | None = None


TypedOptsPatch = Annotated[MolOptsPatch | GroupOptsPatch | ReactionSchemeOptsPatch | EdgeOptsPatch | TextOptsPatch, Field(discriminator='type')]


NodeType = Literal['mol', 'group', 'reaction_scheme', 'edge', 'text']


class ForTypesPatch(StrictModel):
    """Multi-kind patch: `{ "for_types": ["mol","group"], …mol opts }`."""
    for_types: list[NodeType]
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    halo: bool | None = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    scale: float | None = None
    shade: ShadeStyle | None = None
    weight: float | None = None


class MolOpts(StrictModel):
    """Mol cascading paint opts — what a mol leaf consumes from the cascade."""
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    halo: bool | None = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    scale: float | None = None
    shade: ShadeStyle | None = None
    weight: float | None = None


OptsPatch = TypedOptsPatch | ForTypesPatch | MolOpts


Opts = OptsPatch | list[OptsPatch]


RegionStyle = Literal['box', 'bezier', 'circle']


class RegionSpec(StrictModel):
    """One region highlight on a mol."""
    atoms: list[int] = Field(default_factory=list, description='Atom indices that must lie inside the region.')
    exclude: list[int] = Field(default_factory=list, description='Atom indices that must stay outside.')
    fill: FillSpec | None = None
    padding: Annotated[float | None, Field(description='Pad (chem-space px) around the included-atom cover. Omit → house contain margin (~0.1 bond). Applies to box / circle / bezier alike.')] = None
    stroke: StrokeSpec | None = None
    style: RegionStyle = 'box'
    text: MolText | None = None
    z: Annotated[int | None, Field(description='Optional z-order override (lower = further back). Default band: region base.')] = None


RegionField = RegionSpec | list[RegionSpec]


class ShadeSpec(StrictModel):
    """Per-atom / per-bond colormap scores (+ legacy window fields on the mol)."""
    atoms: list[float] | None = None
    bonds: list[float] | None = None
    colormap: Annotated[str | None, Field(description='Named colormap. Only ``"xenosite"`` (default) is painted; any other name is rejected at depict time (not silently ignored).')] = None
    vmax: float = 1.0
    vmin: float = 0.0


MolNodeKind = Literal['mol']


class MolNode(StrictModel):
    """Mol node — [`MolOpts`] fields + non-cascading identity / scores / align."""
    type: Literal['mol'] = 'mol'
    align_to: Annotated[AlignTo | None, Field(description='Template id string, or `{ "ref", "atom_map"?, "min_atoms"?, "mode"?, "timeout_ms"? }`.')] = None
    aromatic: Annotated[AromaticStyle | None, Field(description='Aromatic ring paint: ``circle`` (default) or ``kekule``.')] = None
    atom_label: Annotated[AtomLabelSpec | None, Field(description='Per-atom arbitrary text map (`{ "1": "α" }` or `{ "labels": {…}, "scale"? }`).')] = None
    atom_number: Annotated[AtomNumberSpec | None, Field(description='Atom index / map-number labels (`"index"` | `"mapno"` | policy object).')] = None
    callout: Annotated[CalloutField | None, Field(description='Callout annotation(s).')] = None
    circle: Annotated[CircleField | None, Field(description='Atom / bond / ring highlight circle(s).')] = None
    color: str | None = None
    cxsmiles: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: Annotated[bool | None, Field(description='Paint dashed debug rectangles for layout boxes (labels / mol / scene).')] = None
    halo: bool | None = None
    id: str | None = None
    label: Annotated[Label | None, Field(description='Caption: [`Label`] (string id, list, or `{id, pos?}` — text-node refs).')] = None
    molfile: str | None = None
    name: Annotated[MolText | None, Field(description='Privileged figure name / caption painted inside the mol SVG (default below; vertical reaction schemes default to the right so the name is not in the arrow corridor).')] = None
    opts: Annotated[Opts | None, Field(description='Cascade patches for this node (list or singleton).')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default (rarely override).')] = None
    region: Annotated[RegionField | None, Field(description='Region highlight(s) — one object or a list.')] = None
    scale: float | None = None
    shade: Annotated[ShadeSpec | None, Field(description='Shade **scores** (+ legacy window); window also cascades via [`Opts`].')] = None
    smiles: str | None = None
    star_labels: list[str | None] | None = None
    weight: float | None = None


EdgeArrow = Literal['forward', 'equilibrium', 'open', 'line']


EdgeRouting = Literal['orthogonal', 'polyline', 'splines']


MolIds = str | list[str]


EdgeNodeKind = Literal['edge']


class EdgeNode(StrictModel):
    """Edge **node** — a reaction / network link between mol ids."""
    type: Literal['edge'] = 'edge'
    sources: Annotated[MolIds, Field(description='Reactant mol id(s) — `"a"` or `["a","b"]` (alias: `source`).')]
    targets: Annotated[MolIds, Field(description='Product mol id(s) — `"c"` or `["c","d"]` (alias: `target`).')]
    arrow: EdgeArrow = 'forward'
    color: str | None = None
    dashed: bool = False
    edge_routing: Annotated[EdgeRouting | None, Field(description='Per-edge override of scheme [`LayoutOpts::edge_routing`].')] = None
    label: Annotated[Label | None, Field(description='Label chrome: id / list / `{id, pos?}` / `{above,below,left,right}`.')] = None
    role: Annotated[str | None, Field(description='Optional semantic role (e.g. enzyme) — not drawn by default.')] = None
    stroke_width: float | None = None

    @model_validator(mode="before")
    @classmethod
    def _coerce_endpoints(cls, data: object) -> object:
        if not isinstance(data, dict):
            return data
        out = dict(data)
        if "sources" not in out and "source" in out:
            out["sources"] = out.pop("source")
        if "targets" not in out and "target" in out:
            out["targets"] = out.pop("target")
        out.pop("source", None)
        out.pop("target", None)
        return out



TextNodeKind = Literal['text']


class TextNode(StrictModel):
    """Text **node** — caption / chrome referenced by mols and edges via id."""
    type: Literal['text'] = 'text'
    text: Annotated[str, Field(description='Display text (markup-capable later).')]
    color: str | None = None
    id: Annotated[str | None, Field(description='Stable id — required when other nodes [`MolNode::label`] / edge lanes ref it.')] = None
    opts: Opts | None = None
    scale: float | None = None


Node = MolNode | EdgeNode | TextNode


class GroupNode(StrictModel):
    type: Literal['group'] = 'group'
    align: bool = Field(default=False, description='When true, later children align onto the first (or each `align_to`).')
    aromatic: AromaticStyle | None = None
    children: list[Node] = Field(default_factory=list, description='Child **nodes** — mol | text (no edges).')
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: bool | None = None
    id: str | None = None
    opts: Annotated[Opts | None, Field(description='Group-level cascade bag (list container for child inheritance).')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default.')] = None
    scale: float | None = None


LayoutAlgorithm = Literal['layered', 'radial', 'force', 'stress']


LayoutDirection = Literal['right', 'left', 'up', 'down']


class LayoutOpts(StrictModel):
    """Layout for [`DepictSpec::ReactionScheme`]."""
    algorithm: Annotated[LayoutAlgorithm | None, Field(description='Placement algorithm (`layered` / `radial` / `force` / `stress`).')] = None
    direction: Annotated[LayoutDirection | None, Field(description='Flow axis (`right` / `left` / `up` / `down`).')] = None
    edge_routing: Annotated[EdgeRouting | None, Field(description='Shaft style (`polyline` / `orthogonal` / `splines`).')] = None
    layer_spacing: Annotated[float | None, Field(description='Between-layer gap along the flow axis (px). Smaller → tighter pack.')] = None
    node_spacing: Annotated[float | None, Field(description='Within-layer node gap (px). Smaller → tighter pack.')] = None


class ReactionSchemeNode(StrictModel):
    """Reaction / pathway scheme: mixed **node** children (`mol` | `edge` | `text`)."""
    type: Literal['reaction_scheme'] = 'reaction_scheme'
    aromatic: AromaticStyle | None = None
    children: list[Node] = Field(default_factory=list, description='Child **nodes** — mols, edges, and text (any order).')
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: bool | None = None
    id: str | None = None
    layout: Annotated[LayoutOpts | None, Field(description='Scheme layout (direction, edge routing); backend maps these.')] = None
    opts: Annotated[Opts | None, Field(description='Scheme-level cascade bag for child mol inheritance.')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default.')] = None
    scale: float | None = None


class GridNode(StrictModel):
    """Grid root — each child is one row (`mol` / `group` / `reaction_scheme` / nested `grid`)."""
    type: Literal['grid'] = 'grid'
    aromatic: AromaticStyle | None = None
    children: list[DepictSpecRoot] = Field(default_factory=list, description='Row roots (nested grids flatten in document order when walking mols).')
    color: str | None = None
    debug_align: Annotated[bool | None, Field(description='Paint alignment correspondence (matched atom tags + mol role badge).')] = None
    debug_box: bool | None = None
    id: str | None = None
    opts: Annotated[Opts | None, Field(description='Grid-level cascade bag (inherits onto every row / mol).')] = None
    pad: Annotated[float | None, Field(description='Post-layout SVG viewBox pad (px). Omit → small house default.')] = None
    scale: float | None = None


DepictSpecRoot = Annotated[MolNode | GroupNode | ReactionSchemeNode | GridNode, Field(discriminator='type')]

class DepictSpec(RootModel[DepictSpecRoot]):
    """Declarative document (`mol`, `group`, `reaction_scheme`, or `grid` root)."""

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault('exclude_none', True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault('exclude_none', True)
        return super().model_dump_json(*args, **kwargs)


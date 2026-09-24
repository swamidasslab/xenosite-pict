# Auto-generated from Rust schemars (make types) — do not edit.
"""DepictSpec — live document ABI from xpict-core (schemars)."""

from __future__ import annotations

from typing import Annotated, Literal, Any

from pydantic import BaseModel, ConfigDict, Field, RootModel


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump_json(*args, **kwargs)

class AlignToSpec(StrictModel):
    """Object form of document ``align_to`` (template ref + EdgePlan-style opts)."""
    ref: Annotated[str, Field(description='Id of the template mol in this group.')]
    atom_map: Annotated[list[tuple[int, int]] | None, Field(description='Pairs `(query, template)` vs the template; skips MCS when set.')] = None
    min_atoms: Annotated[int | None, Field(description='Override [`crate::edge::MIN_MCS_ATOMS`] when set.')] = None


AlignTo = str | AlignToSpec


LabelPos = Literal['above', 'below', 'left', 'right']


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
    colormap: str | None = None
    vmax: float | None = None
    vmin: float | None = None


class MolOptsPatch(StrictModel):
    """Mol cascading paint opts — what a mol leaf consumes from the cascade."""
    type: Literal['mol'] = 'mol'
    color: str | None = None
    halo: bool | None = None
    scale: float | None = None
    shade: ShadeStyle | None = None
    weight: float | None = None


class GroupOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['group'] = 'group'
    color: str | None = None
    scale: float | None = None


class ReactionSchemeOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['reaction_scheme'] = 'reaction_scheme'
    color: str | None = None
    scale: float | None = None


class EdgeOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['edge'] = 'edge'
    color: str | None = None
    scale: float | None = None


class TextOptsPatch(StrictModel):
    """Universal cascading keys (any node kind)."""
    type: Literal['text'] = 'text'
    color: str | None = None
    scale: float | None = None


TypedOptsPatch = Annotated[MolOptsPatch | GroupOptsPatch | ReactionSchemeOptsPatch | EdgeOptsPatch | TextOptsPatch, Field(discriminator='type')]


NodeType = Literal['mol', 'group', 'reaction_scheme', 'edge', 'text']


class ForTypesPatch(StrictModel):
    """Multi-kind patch: `{ "for_types": ["mol","group"], …common opts }`."""
    for_types: list[NodeType]
    color: str | None = None
    scale: float | None = None


class CommonOpts(StrictModel):
    """Universal cascading keys (any node kind)."""
    color: str | None = None
    scale: float | None = None


OptsPatch = TypedOptsPatch | ForTypesPatch | CommonOpts


Opts = OptsPatch | list[OptsPatch]


class ShadeSpec(StrictModel):
    """Per-atom / per-bond colormap scores (+ legacy window fields on the mol)."""
    atoms: list[float] | None = None
    bonds: list[float] | None = None
    colormap: str | None = None
    vmax: float = 1.0
    vmin: float = 0.0


MolNodeKind = Literal['mol']


class MolNode(StrictModel):
    """Mol node — [`MolOpts`] fields + non-cascading identity / scores / align."""
    type: Literal['mol'] = 'mol'
    align_to: Annotated[AlignTo | None, Field(description='Template id string, or `{ "ref", "atom_map"?, "min_atoms"? }`.')] = None
    color: str | None = None
    cxsmiles: str | None = None
    halo: bool | None = None
    id: str | None = None
    label: Annotated[Label | None, Field(description='Caption: [`Label`] (string id, list, or `{id, pos?}` — text-node refs).')] = None
    molfile: str | None = None
    opts: Annotated[Opts | None, Field(description='Cascade patches for this node (list or singleton).')] = None
    scale: float | None = None
    shade: Annotated[ShadeSpec | None, Field(description='Shade **scores** (+ legacy window); window also cascades via [`Opts`].')] = None
    smiles: str | None = None
    star_labels: list[str | None] | None = None
    weight: float | None = None


EdgeArrow = Literal['forward', 'equilibrium', 'open', 'line']


EdgeNodeKind = Literal['edge']


class EdgeNode(StrictModel):
    """Edge **node** — a reaction / network link between mol ids."""
    type: Literal['edge'] = 'edge'
    source: Annotated[str, Field(description='Id of the source mol node.')]
    target: Annotated[str, Field(description='Id of the target mol node.')]
    arrow: EdgeArrow = 'forward'
    color: str | None = None
    dashed: bool = False
    label: Annotated[Label | None, Field(description='Label chrome: id / list / `{id, pos?}` / `{above,below,left,right}`.')] = None
    role: Annotated[str | None, Field(description='Optional semantic role (e.g. enzyme) — not drawn by default.')] = None
    stroke_width: float | None = None


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
    children: list[Node] = Field(default_factory=list, description='Child **nodes** — mol | text (no edges).')
    color: str | None = None
    id: str | None = None
    opts: Annotated[Opts | None, Field(description='Group-level cascade bag (list container for child inheritance).')] = None
    scale: float | None = None


ElkDirection = Literal['RIGHT', 'LEFT', 'UP', 'DOWN']


ElkEdgeRouting = Literal['ORTHOGONAL', 'POLYLINE', 'SPLINES']


class LayoutOpts(StrictModel):
    """Layout for [`DepictSpec::ReactionScheme`] — **ELK layered by default**."""
    direction: Annotated[ElkDirection | None, Field(description='Flow axis (`RIGHT` / `LEFT` / `UP` / `DOWN`).')] = None
    edge_routing: Annotated[ElkEdgeRouting | None, Field(description='Shaft routing (`POLYLINE` / `ORTHOGONAL` / `SPLINES`).')] = None
    elk_options: Annotated[dict[str, Any] | None, Field(description='Extra ELK layout options (string values), e.g. spacing overrides.')] = None


class ReactionSchemeNode(StrictModel):
    """Reaction / pathway scheme: mixed **node** children (`mol` | `edge` | `text`)."""
    type: Literal['reaction_scheme'] = 'reaction_scheme'
    children: list[Node] = Field(default_factory=list, description='Child **nodes** — mols, edges, and text (any order).')
    color: str | None = None
    id: str | None = None
    layout: Annotated[LayoutOpts | None, Field(description='ELK layout (defaults applied when omitted).')] = None
    opts: Annotated[Opts | None, Field(description='Scheme-level cascade bag for child mol inheritance.')] = None
    scale: float | None = None


DepictSpecRoot = Annotated[MolNode | GroupNode | ReactionSchemeNode, Field(discriminator='type')]

class DepictSpec(RootModel[DepictSpecRoot]):
    """Declarative document (`mol`, `group`, or `reaction_scheme` root)."""

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault('exclude_none', True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault('exclude_none', True)
        return super().model_dump_json(*args, **kwargs)


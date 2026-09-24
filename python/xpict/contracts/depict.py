# Auto-generated from Rust schemars (make types) — do not edit.
"""DepictSpec — live document ABI from xpict-core (schemars)."""

from __future__ import annotations

from typing import Annotated, Literal

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


TypedOptsPatch = Annotated[MolOptsPatch | GroupOptsPatch | ReactionSchemeOptsPatch | EdgeOptsPatch, Field(discriminator='type')]


NodeType = Literal['mol', 'group', 'reaction_scheme', 'edge']


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
    molfile: str | None = None
    opts: Annotated[Opts | None, Field(description='Cascade patches for this node (list or singleton).')] = None
    scale: float | None = None
    shade: Annotated[ShadeSpec | None, Field(description='Shade **scores** (+ legacy window); window also cascades via [`Opts`].')] = None
    smiles: str | None = None
    star_labels: list[str | None] | None = None
    weight: float | None = None


class GroupNode(StrictModel):
    type: Literal['group'] = 'group'
    align: bool = Field(default=False, description='When true, later children align onto the first (or each `align_to`).')
    children: list[MolNode] = Field(default_factory=list, description='Child mols only (no edges).')
    color: str | None = None
    id: str | None = None
    opts: Annotated[Opts | None, Field(description='Group-level cascade bag (list container for child inheritance).')] = None
    scale: float | None = None


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
    label: str | None = None
    role: Annotated[str | None, Field(description='Optional semantic role (e.g. enzyme) — not drawn by default.')] = None
    stroke_width: float | None = None


Node = MolNode | EdgeNode


class ReactionSchemeNode(StrictModel):
    """Reaction / pathway scheme: mixed **node** children (`mol` | `edge`)."""
    type: Literal['reaction_scheme'] = 'reaction_scheme'
    children: list[Node] = Field(default_factory=list, description='Child **nodes** — mols and edges interleaved (or any order).')
    color: str | None = None
    id: str | None = None
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


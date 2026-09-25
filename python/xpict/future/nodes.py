"""Nested figure object algebra — canonical PictSpec tree.

Every node has ``type``, optional ``id`` / ``panel`` / ``layout``.
Containers (``group`` / ``grid`` / ``stack`` / ``reaction`` / ``network``)
also have ``children``. Leaves (``mol``, ``arrow``, ``image``, …) do not —
molecule chrome stays on mol fields (``annotations``, ``rtable``, …).

Legacy flat ``{molecules, diagram}`` documents are lifted on validate and can
be flattened back for the current render pipeline.
"""

from __future__ import annotations

from typing import Annotated, Any, Literal

from pydantic import (
    Field,
    RootModel,
    TypeAdapter,
    model_validator,
)

from xpict.future.shorthand import (
    expand_pict_input,
)
from xpict.future.spec import (
    AnnotationSpec,
    AnnotKind,
    AnnotPrefer,
    DiagramKind,
    DiagramSpec,
    EdgeArrow,
    EdgeSpec,
    LegacyPictSpec,
    MarkSpec,
    MoleculeSpec,
    RingAttachmentSpec,
    ShadeSpec,
    StrictModel,
    _LabelInput,
    _RGroupsInput,
    _RTableInput,
)

# ---------------------------------------------------------------------------
# Layout (hierarchical; document chrome lives here on the root)
# ---------------------------------------------------------------------------


class LayoutSpec(StrictModel):
    """How this node places itself and/or its children."""

    # Container flow
    columns: int | None = Field(default=None, description="Grid columns")
    direction: Literal["row", "column"] | None = Field(
        default=None, description="Stack / reaction flow direction"
    )
    gap: float | None = Field(default=None, description="Spacing between children")
    node_spacing: float | None = Field(
        default=None,
        description="Within-layer node gap (px); smaller packs tighter",
    )
    layer_spacing: float | None = Field(
        default=None,
        description="Between-layer gap along flow (px); smaller packs tighter",
    )
    attach: Literal["below", "above", "left", "right"] | None = Field(
        default=None,
        description="Where a nested child attaches relative to its parent",
    )

    # Document / viewport chrome
    width: float | None = None
    height: float | None = None
    halo: bool | None = Field(
        default=None,
        description="White knockout under ink; typically set on the root",
    )
    align: bool = Field(
        default=False,
        description="Align later molecules onto the first (legacy diagram.align)",
    )
    elk_options: dict[str, Any] = Field(default_factory=dict)

    # Multi-image / dump projection
    output: Literal["root", "children", "leaves", "all"] = Field(
        default="root",
        description="Render projection: whole tree vs explode children/leaves",
    )

    # Captions / arrow chrome (strings or nested node dicts)
    label: _LabelInput = Field(
        default=None,
        description="Molecule caption (string shorthand or LabelSpec)",
    )
    caption: str | None = Field(default=None, description="Image / panel caption")
    above: list[Any] = Field(
        default_factory=list,
        description="Arrow / edge content above the shaft (text or nodes)",
    )
    below: list[Any] = Field(
        default_factory=list,
        description="Arrow / edge content below the shaft (text or nodes)",
    )

    # Non-tree links (network) or lifted legacy edges
    edges: list[EdgeSpec] = Field(
        default_factory=list,
        description="Explicit edges when children are not a linear reaction walk",
    )


# ---------------------------------------------------------------------------
# Nodes
# ---------------------------------------------------------------------------


class NodeCommon(StrictModel):
    """Fields shared by every figure node (factored to ``allOf`` in JSON Schema)."""

    id: str | None = Field(default=None, description="Optional stable id for refs/edges")
    panel: str | None = Field(default=None, description="Subfigure tag drawn as (a), (b), …")
    layout: LayoutSpec = Field(default_factory=LayoutSpec)
    meta: dict[str, Any] = Field(default_factory=dict)


class ContainerCommon(NodeCommon):
    """Node that owns nested children (group / grid / stack / reaction / network)."""

    children: list[Node] = Field(
        default_factory=list,
        description="Owned nested nodes (same discriminated union as the document root)",
    )


class AlignToSpec(StrictModel):
    """Object form of document ``align_to`` (template ref + align opts)."""

    ref: str = Field(description="Id of the template mol in this group")
    atom_map: list[tuple[int, int]] | None = Field(
        default=None,
        description=(
            "Pairs (query_atom, template_atom) vs the template. Skips MCS when set."
        ),
    )
    min_atoms: int | None = Field(
        default=None,
        description="Override minimum mapped atoms before align is trusted",
        ge=1,
    )


class MolNode(NodeCommon):
    """Molecule object (chemistry payload + optional nested annotations/tables)."""

    type: Literal["mol"] = "mol"
    smiles: str | None = None
    cxsmiles: str | None = None
    esmiles: str | None = None
    molfile: str | None = None
    ids: dict[str, int | list[int]] = Field(default_factory=dict)
    rings: dict[str, list[int]] = Field(default_factory=dict)
    rgroups: _RGroupsInput = None
    star_labels: list[str | None] | None = None
    align_to: str | AlignToSpec | None = Field(
        default=None,
        description=(
            "Align template: id string, or object "
            "``{ref, atom_map?, min_atoms?}``. "
            "When omitted and the group has align=true, defaults to the first child."
        ),
    )
    ring_attachments: list[RingAttachmentSpec] = Field(default_factory=list)
    rtable: _RTableInput = None
    marks: list[MarkSpec] = Field(default_factory=list)
    annotations: list[AnnotationSpec] = Field(default_factory=list)
    shade: ShadeSpec | None = None
    color: str | None = None
    scale: float = Field(
        default=1.0,
        description="Uniform diagram scale (font, stroke, geometry). 1.0 = house size.",
        gt=0,
    )
    weight: float = Field(
        default=1.0,
        description=(
            "Ink weight relative to house size (1.0). "
            "May go down to 2/3 (Regular stem); typical thicken up to ~2."
        ),
        ge=2.0 / 3.0,
    )

    @model_validator(mode="before")
    @classmethod
    def _alias_title(cls, data: Any) -> Any:
        if isinstance(data, dict) and "title" in data:
            data = dict(data)
            layout = dict(data.get("layout") or {})
            if "label" not in layout or layout.get("label") is None:
                if data.get("label") is None:
                    layout["label"] = data["title"]
                    data["layout"] = layout
                elif "label" not in data or data["label"] is None:
                    data["label"] = data["title"]
            del data["title"]
        # Promote bare mol.label → layout.label
        if isinstance(data, dict) and "label" in data and data["label"] is not None:
            data = dict(data)
            layout = dict(data.get("layout") or {})
            if layout.get("label") is None:
                layout["label"] = data.pop("label")
                data["layout"] = layout
            else:
                data.pop("label", None)
        return data

    @model_validator(mode="after")
    def require_structure(self) -> MolNode:
        if not any([self.smiles, self.cxsmiles, self.esmiles, self.molfile]):
            raise ValueError("mol node requires smiles, cxsmiles, esmiles, or molfile")
        overlap = set(self.ids) & set(self.rings)
        if overlap:
            raise ValueError(
                f"mol ids/rings names must be unique in scope; duplicates: {sorted(overlap)}"
            )
        for ra in self.ring_attachments:
            if isinstance(ra.ring, str) and ra.ring not in self.rings:
                raise ValueError(
                    f"ring_attachment label={ra.label!r} references unknown "
                    f"ring {ra.ring!r}; declare it in rings"
                )
        return self


class ArrowNode(NodeCommon):
    """Reaction / diagram arrow between siblings (or via layout.edges)."""

    type: Literal["arrow"] = "arrow"
    arrow: EdgeArrow = EdgeArrow.forward
    label: str | None = Field(
        default=None,
        description="Legacy single edge label; prefer layout.above",
    )
    role: str | None = None
    color: str | None = None
    stroke_width: float | None = None
    dashed: bool = False


class TextNode(NodeCommon):
    type: Literal["text"] = "text"
    text: str


class ImageNode(NodeCommon):
    type: Literal["image"] = "image"
    src: str
    alt: str | None = None


class TableNode(NodeCommon):
    """Tabular panel; cells may be strings or nested nodes (e.g. mol)."""

    type: Literal["table"] = "table"
    columns: list[str] | None = None
    rows: list[list[Any]] = Field(default_factory=list)


class RefNode(NodeCommon):
    """Reference to another node by id (sharing)."""

    type: Literal["ref"] = "ref"
    ref: str


class AnnotationNode(NodeCommon):
    """Standalone annotation node (prefer ``mol.annotations`` when targeting a mol)."""

    type: Literal["annotation"] = "annotation"
    kind: AnnotKind = AnnotKind.callout
    atoms: list[int] | None = None
    bonds: list[tuple[int, int]] | None = None
    ring: list[int] | None = None
    label: str | None = None
    color: str | None = None
    arrow: bool = True
    prefer: AnnotPrefer = AnnotPrefer.auto

    @model_validator(mode="after")
    def require_target(self) -> AnnotationNode:
        if not any([self.atoms, self.bonds, self.ring]):
            raise ValueError("annotation node needs atoms, bonds, or ring")
        return self


class GroupNode(ContainerCommon):
    type: Literal["group"] = "group"
    align: bool = Field(
        default=False,
        description=(
            "When true, later children align onto the first (or each child's "
            "align_to id). Builds an EdgePlan coord_gen forest."
        ),
    )


class GridNode(ContainerCommon):
    type: Literal["grid"] = "grid"


class StackNode(ContainerCommon):
    type: Literal["stack"] = "stack"


class ReactionNode(ContainerCommon):
    type: Literal["reaction"] = "reaction"


class NetworkNode(ContainerCommon):
    type: Literal["network"] = "network"


Node = Annotated[
    MolNode
    | ArrowNode
    | TextNode
    | ImageNode
    | TableNode
    | RefNode
    | AnnotationNode
    | GroupNode
    | GridNode
    | StackNode
    | ReactionNode
    | NetworkNode,
    Field(discriminator="type"),
]

# Rebuild forward refs for children: list[Node]
for _cls in (
    NodeCommon,
    ContainerCommon,
    MolNode,
    ArrowNode,
    TextNode,
    ImageNode,
    TableNode,
    RefNode,
    AnnotationNode,
    GroupNode,
    GridNode,
    StackNode,
    ReactionNode,
    NetworkNode,
):
    _cls.model_rebuild()

_NODE_ADAPTER: TypeAdapter[Node] = TypeAdapter(Node)


# ---------------------------------------------------------------------------
# Legacy ↔ nested
# ---------------------------------------------------------------------------

_KIND_TO_TYPE = {
    DiagramKind.single: "group",
    DiagramKind.grid: "grid",
    DiagramKind.network: "network",
    DiagramKind.reaction: "reaction",
    DiagramKind.html: "group",
}


def is_legacy_pict(data: Any) -> bool:
    """True when input looks like flat ``{molecules, diagram}``."""
    return isinstance(data, dict) and "molecules" in data and "type" not in data


def lift_legacy(data: dict[str, Any]) -> dict[str, Any]:
    """Flat PictSpec JSON → nested root node dict."""
    root = expand_pict_input(dict(data))
    mols = list(root.get("molecules") or [])
    diagram = root.get("diagram") or {}
    if isinstance(diagram, DiagramSpec):
        diagram = diagram.model_dump(mode="json")
    kind_raw = diagram.get("kind", "single")
    try:
        kind = DiagramKind(kind_raw)
    except ValueError:
        kind = DiagramKind.single
    node_type = _KIND_TO_TYPE.get(kind, "group")

    layout: dict[str, Any] = {
        "halo": root.get("halo", True),
        "width": root.get("width"),
        "height": root.get("height"),
        "columns": diagram.get("columns"),
        "align": bool(diagram.get("align", False)),
        "node_spacing": diagram.get("node_spacing"),
        "layer_spacing": diagram.get("layer_spacing"),
        "elk_options": dict(diagram.get("elk_options") or {}),
        "edges": list(diagram.get("edges") or []),
    }

    children: list[dict[str, Any]] = []
    for mol in mols:
        if not isinstance(mol, dict):
            continue
        child = dict(mol)
        child["type"] = "mol"
        label = child.pop("label", None)
        title = child.pop("title", None)
        if label is None:
            label = title
        if label is not None:
            child["layout"] = dict(child.get("layout") or {})
            child["layout"]["label"] = label
        children.append(child)

    meta = dict(root.get("meta") or {})

    if (
        node_type == "group"
        and len(children) == 1
        and not layout["edges"]
        and layout.get("columns") is None
    ):
        only = children[0]
        only_layout = dict(only.get("layout") or {})
        for key in ("halo", "width", "height"):
            if layout.get(key) is not None and only_layout.get(key) is None:
                only_layout[key] = layout[key]
        only["layout"] = only_layout
        if meta:
            only["meta"] = meta
        return only

    return {
        "type": node_type,
        "layout": layout,
        "children": children,
        "meta": meta,
    }


def _ensure_mol_id(node: MolNode, used: set[str], index: int) -> str:
    if node.id:
        used.add(node.id)
        return node.id
    base = f"m{index}"
    name = base
    n = 1
    while name in used:
        name = f"{base}_{n}"
        n += 1
    used.add(name)
    return name


def mol_to_molecule_spec(node: MolNode) -> MoleculeSpec:
    """Project a mol node to the chemistry MoleculeSpec used by backends."""
    return MoleculeSpec(
        id=node.id,
        smiles=node.smiles,
        cxsmiles=node.cxsmiles,
        esmiles=node.esmiles,
        molfile=node.molfile,
        label=node.layout.label,
        ids=node.ids,
        rings=node.rings,
        rgroups=node.rgroups,
        star_labels=node.star_labels,
        ring_attachments=node.ring_attachments,
        rtable=node.rtable,
        marks=node.marks,
        annotations=list(node.annotations),
        shade=node.shade,
        color=node.color,
        scale=node.scale,
        weight=node.weight,
    )


def _arrow_to_edge(arrow: ArrowNode, source: str, target: str) -> EdgeSpec:
    label = arrow.label
    label_pos: str = "above"
    if label is None and arrow.layout.above:
        first = arrow.layout.above[0]
        label = first if isinstance(first, str) else None
        label_pos = "above"
    elif label is None and arrow.layout.below:
        first = arrow.layout.below[0]
        label = first if isinstance(first, str) else None
        label_pos = "below"
    return EdgeSpec(
        sources=[source],
        targets=[target],
        label=label,
        label_pos=label_pos,  # type: ignore[arg-type]
        role=arrow.role,
        arrow=arrow.arrow,
        color=arrow.color,
        stroke_width=arrow.stroke_width,
        dashed=arrow.dashed,
    )


def flatten_to_legacy(node: Node) -> LegacyPictSpec:
    """Nested tree → flat ``{molecules, diagram}`` for the render pipeline."""
    molecules: list[MoleculeSpec] = []
    edges: list[EdgeSpec] = []
    used_ids: set[str] = set()
    mol_index = 0

    root_layout = node.layout
    kind = DiagramKind.single
    columns = root_layout.columns
    align = root_layout.align
    elk_options = dict(root_layout.elk_options)
    halo = True if root_layout.halo is None else root_layout.halo
    width = root_layout.width
    height = root_layout.height
    meta = dict(node.meta)

    match node:
        case GridNode():
            kind = DiagramKind.grid
        case ReactionNode():
            kind = DiagramKind.reaction
        case NetworkNode():
            kind = DiagramKind.network
        case GroupNode() | StackNode() | MolNode():
            kind = DiagramKind.single
        case _:
            pass

    def walk_container(n: Node) -> None:
        nonlocal mol_index
        edges.extend(list(n.layout.edges))

        match n:
            case ReactionNode() | GroupNode() | StackNode() | GridNode() | NetworkNode():
                pending_arrow: ArrowNode | None = None
                prev_id: str | None = None
                # When layout.edges already lists links (legacy lift), do not also
                # synthesize a chain between consecutive mol children.
                synthesize = isinstance(n, ReactionNode) and not n.layout.edges
                for child in n.children:
                    match child:
                        case MolNode():
                            mid = _ensure_mol_id(child, used_ids, mol_index)
                            mol_index += 1
                            # assign id onto a copy via MoleculeSpec
                            spec = mol_to_molecule_spec(child)
                            if spec.id is None:
                                spec = spec.model_copy(update={"id": mid})
                            molecules.append(spec)
                            if synthesize and prev_id is not None:
                                if pending_arrow is not None:
                                    edges.append(_arrow_to_edge(pending_arrow, prev_id, mid))
                                    pending_arrow = None
                                else:
                                    edges.append(
                                        EdgeSpec(
                                            sources=[prev_id],
                                            targets=[mid],
                                            arrow=EdgeArrow.forward,
                                        )
                                    )
                            prev_id = mid
                        case ArrowNode():
                            pending_arrow = child
                        case (
                            GroupNode() | StackNode() | GridNode() | ReactionNode() | NetworkNode()
                        ):
                            walk_container(child)
                        case TableNode():
                            # Tables are not yet drawn; Markush rtable already on mols.
                            continue
                        case _:
                            # ImageNode | TextNode | RefNode | AnnotationNode — not layout.
                            continue
                return

            case MolNode():
                mid = _ensure_mol_id(n, used_ids, mol_index)
                mol_index += 1
                spec = mol_to_molecule_spec(n)
                if spec.id is None:
                    spec = spec.model_copy(update={"id": mid})
                molecules.append(spec)

            case _:
                pass

    match node:
        case GroupNode() | StackNode() | GridNode() | ReactionNode() | NetworkNode() | MolNode():
            walk_container(node)
        case _:
            # Non-mol root with no mol children — empty legacy (invalid for render)
            pass

    if not molecules:
        raise ValueError("figure has no mol nodes to render")

    return LegacyPictSpec(
        molecules=molecules,
        diagram=DiagramSpec(
            kind=kind,
            columns=columns,
            edges=edges,
            node_spacing=root_layout.node_spacing,
            layer_spacing=root_layout.layer_spacing
            if root_layout.layer_spacing is not None
            else root_layout.gap,
            elk_options=elk_options,
            align=align,
        ),
        width=width,
        height=height,
        halo=halo,
        meta=meta,
    )


class PictSpec(RootModel[Node]):
    """Canonical depiction document: one nested node tree.

    Also accepts legacy flat ``{molecules, diagram, …}``, which is lifted to a
    nested root on validation. Use :meth:`to_legacy` for the render pipeline.
    """

    root: Node

    @model_validator(mode="before")
    @classmethod
    def _lift_legacy(cls, data: Any) -> Any:
        if is_legacy_pict(data):
            return lift_legacy(data)
        return data

    def to_legacy(self) -> LegacyPictSpec:
        """Flatten to the molecule-list document the draw/ELK path expects."""
        return flatten_to_legacy(self.root)

    @property
    def molecules(self) -> list[MoleculeSpec]:
        """Legacy-compatible molecule list (flattened)."""
        return self.to_legacy().molecules

    @property
    def diagram(self) -> DiagramSpec:
        return self.to_legacy().diagram

    @property
    def halo(self) -> bool:
        return self.to_legacy().halo

    @property
    def width(self) -> float | None:
        return self.to_legacy().width

    @property
    def height(self) -> float | None:
        return self.to_legacy().height

    @property
    def meta(self) -> dict[str, Any]:
        return dict(self.root.meta)


def parse_node(data: Node | dict[str, Any]) -> Node:
    """Validate a nested node (no legacy lift)."""
    if isinstance(
        data,
        (
            MolNode,
            ArrowNode,
            TextNode,
            ImageNode,
            TableNode,
            RefNode,
            AnnotationNode,
            GroupNode,
            GridNode,
            StackNode,
            ReactionNode,
            NetworkNode,
        ),
    ):
        return data
    return _NODE_ADAPTER.validate_python(data)


def expand_pict(spec: PictSpec | LegacyPictSpec | dict[str, Any] | Node) -> PictSpec:
    """Validate nested or legacy input into a :class:`PictSpec` tree."""
    if isinstance(spec, PictSpec):
        return spec
    if isinstance(spec, LegacyPictSpec):
        return PictSpec.model_validate(lift_legacy(spec.model_dump(mode="json")))
    if isinstance(
        spec,
        (
            MolNode,
            ArrowNode,
            TextNode,
            ImageNode,
            TableNode,
            RefNode,
            AnnotationNode,
            GroupNode,
            GridNode,
            StackNode,
            ReactionNode,
            NetworkNode,
        ),
    ):
        return PictSpec(root=spec)
    return PictSpec.model_validate(spec)


def compress_pict(spec: PictSpec | LegacyPictSpec | dict[str, Any]) -> dict[str, Any]:
    """Dump to JSON-shaped dict (nested). Legacy inputs are lifted first."""
    from xpict.future.shorthand import compress_label

    tree = expand_pict(spec)
    data = tree.model_dump(mode="json")

    def _compress(node: Any) -> Any:
        if not isinstance(node, dict):
            return node
        out = dict(node)
        layout = out.get("layout")
        if isinstance(layout, dict) and "label" in layout:
            layout = dict(layout)
            layout["label"] = compress_label(layout["label"])
            out["layout"] = layout
        if "children" in out and isinstance(out["children"], list):
            out["children"] = [_compress(c) for c in out["children"]]
        return out

    return _compress(data)


__all__ = [
    "AnnotationNode",
    "ArrowNode",
    "GridNode",
    "GroupNode",
    "ImageNode",
    "LayoutSpec",
    "MolNode",
    "NetworkNode",
    "Node",
    "NodeCommon",
    "ContainerCommon",
    "PictSpec",
    "ReactionNode",
    "RefNode",
    "StackNode",
    "TableNode",
    "TextNode",
    "compress_pict",
    "expand_pict",
    "flatten_to_legacy",
    "is_legacy_pict",
    "lift_legacy",
    "mol_to_molecule_spec",
    "parse_node",
]

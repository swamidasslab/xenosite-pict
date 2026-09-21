"""Nested figure object algebra — canonical PictSpec tree.

Every node has ``type``, optional ``id`` / ``panel`` / ``layout`` / ``children``.
Legacy flat ``{molecules, diagram}`` documents are lifted on validate and can be
flattened back for the current render pipeline.
"""

from __future__ import annotations

from typing import Annotated, Any, Literal, Union

from pydantic import (
    Field,
    RootModel,
    TypeAdapter,
    model_validator,
)

from xpict.contracts.shorthand import (
    expand_pict_input,
)
from xpict.contracts.spec import (
    AnnotKind,
    AnnotPrefer,
    AnnotationSpec,
    DiagramKind,
    DiagramSpec,
    EdgeArrow,
    EdgeSpec,
    LegacyPictSpec,
    MarkSpec,
    MoleculeSpec,
    RingAttachmentSpec,
    RTableSpec,
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


class _NodeCommon(StrictModel):
    id: str | None = Field(default=None, description="Optional stable id for refs/edges")
    panel: str | None = Field(
        default=None, description="Subfigure tag drawn as (a), (b), …"
    )
    layout: LayoutSpec = Field(default_factory=LayoutSpec)
    meta: dict[str, Any] = Field(default_factory=dict)


class MolNode(_NodeCommon):
    """Molecule object (chemistry payload + optional nested annotations/tables)."""

    type: Literal["mol"] = "mol"
    smiles: str | None = None
    cxsmiles: str | None = None
    esmiles: str | None = None
    molfile: str | None = None
    ids: dict[str, int | list[int]] = Field(default_factory=dict)
    rings: dict[str, list[int]] = Field(default_factory=dict)
    rgroups: _RGroupsInput = None
    ring_attachments: list[RingAttachmentSpec] = Field(default_factory=list)
    rtable: _RTableInput = None
    marks: list[MarkSpec] = Field(default_factory=list)
    annotations: list[AnnotationSpec] = Field(default_factory=list)
    shade: ShadeSpec | None = None
    color: str | None = None
    children: list[Node] = Field(default_factory=list)

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
                f"mol ids/rings names must be unique in scope; "
                f"duplicates: {sorted(overlap)}"
            )
        for ra in self.ring_attachments:
            if isinstance(ra.ring, str) and ra.ring not in self.rings:
                raise ValueError(
                    f"ring_attachment label={ra.label!r} references unknown "
                    f"ring {ra.ring!r}; declare it in rings"
                )
        return self


class ArrowNode(_NodeCommon):
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
    children: list[Node] = Field(default_factory=list)


class TextNode(_NodeCommon):
    type: Literal["text"] = "text"
    text: str
    children: list[Node] = Field(default_factory=list)


class ImageNode(_NodeCommon):
    type: Literal["image"] = "image"
    src: str
    alt: str | None = None
    children: list[Node] = Field(default_factory=list)


class TableNode(_NodeCommon):
    """Tabular panel; cells may be strings or nested nodes (e.g. mol)."""

    type: Literal["table"] = "table"
    columns: list[str] | None = None
    rows: list[list[Any]] = Field(default_factory=list)
    children: list[Node] = Field(default_factory=list)


class RefNode(_NodeCommon):
    """Reference to another node by id (sharing)."""

    type: Literal["ref"] = "ref"
    ref: str
    children: list[Node] = Field(default_factory=list)


class AnnotationNode(_NodeCommon):
    """Annotation as a nested object (alternative to mol.annotations list)."""

    type: Literal["annotation"] = "annotation"
    kind: AnnotKind = AnnotKind.callout
    atoms: list[int] | None = None
    bonds: list[tuple[int, int]] | None = None
    ring: list[int] | None = None
    label: str | None = None
    color: str | None = None
    arrow: bool = True
    prefer: AnnotPrefer = AnnotPrefer.auto
    children: list[Node] = Field(default_factory=list)

    @model_validator(mode="after")
    def require_target(self) -> AnnotationNode:
        if not any([self.atoms, self.bonds, self.ring]):
            raise ValueError("annotation node needs atoms, bonds, or ring")
        return self


class GroupNode(_NodeCommon):
    type: Literal["group"] = "group"
    children: list[Node] = Field(default_factory=list)


class GridNode(_NodeCommon):
    type: Literal["grid"] = "grid"
    children: list[Node] = Field(default_factory=list)


class StackNode(_NodeCommon):
    type: Literal["stack"] = "stack"
    children: list[Node] = Field(default_factory=list)


class ReactionNode(_NodeCommon):
    type: Literal["reaction"] = "reaction"
    children: list[Node] = Field(default_factory=list)


class NetworkNode(_NodeCommon):
    type: Literal["network"] = "network"
    children: list[Node] = Field(default_factory=list)


Node = Annotated[
    Union[
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
    ],
    Field(discriminator="type"),
]

# Rebuild forward refs for children: list[Node]
for _cls in (
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
    return (
        isinstance(data, dict)
        and "molecules" in data
        and "type" not in data
    )


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
    label = node.layout.label
    # Nested annotation nodes → AnnotationSpec list
    annotations = list(node.annotations)
    rtable = node.rtable
    for child in node.children:
        if isinstance(child, AnnotationNode):
            annotations.append(
                AnnotationSpec(
                    kind=child.kind,
                    atoms=child.atoms,
                    bonds=child.bonds,
                    ring=child.ring,
                    label=child.label,
                    color=child.color,
                    arrow=child.arrow,
                    prefer=child.prefer,
                )
            )
        elif isinstance(child, TableNode) and rtable is None:
            rtable = RTableSpec(groups=child.columns, rows=_stringify_rows(child.rows))
    return MoleculeSpec(
        id=node.id,
        smiles=node.smiles,
        cxsmiles=node.cxsmiles,
        esmiles=node.esmiles,
        molfile=node.molfile,
        label=label,
        ids=node.ids,
        rings=node.rings,
        rgroups=node.rgroups,
        ring_attachments=node.ring_attachments,
        rtable=rtable,
        marks=node.marks,
        annotations=annotations,
        shade=node.shade,
        color=node.color,
    )


def _stringify_rows(rows: list[list[Any]]) -> list[list[str]]:
    out: list[list[str]] = []
    for row in rows:
        cells: list[str] = []
        for cell in row:
            if isinstance(cell, str):
                cells.append(cell)
            elif isinstance(cell, dict) and cell.get("smiles"):
                cells.append(str(cell["smiles"]))
            elif isinstance(cell, MolNode) and cell.smiles:
                cells.append(cell.smiles)
            else:
                cells.append(str(cell))
        out.append(cells)
    return out


def _arrow_to_edge(arrow: ArrowNode, source: str, target: str) -> EdgeSpec:
    label = arrow.label
    if label is None and arrow.layout.above:
        first = arrow.layout.above[0]
        label = first if isinstance(first, str) else None
    return EdgeSpec(
        source=source,
        target=target,
        label=label,
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

    if isinstance(node, GridNode):
        kind = DiagramKind.grid
    elif isinstance(node, ReactionNode):
        kind = DiagramKind.reaction
    elif isinstance(node, NetworkNode):
        kind = DiagramKind.network
    elif isinstance(node, (GroupNode, StackNode)):
        kind = DiagramKind.single
    elif isinstance(node, MolNode):
        kind = DiagramKind.single

    def walk_container(n: Node) -> None:
        nonlocal mol_index
        edges.extend(list(n.layout.edges))

        if isinstance(n, (ReactionNode, GroupNode, StackNode, GridNode, NetworkNode)):
            pending_arrow: ArrowNode | None = None
            prev_id: str | None = None
            # When layout.edges already lists links (legacy lift), do not also
            # synthesize a chain between consecutive mol children.
            synthesize = isinstance(n, ReactionNode) and not n.layout.edges
            for child in n.children:
                if isinstance(child, MolNode):
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
                                    source=prev_id,
                                    target=mid,
                                    arrow=EdgeArrow.forward,
                                )
                            )
                    prev_id = mid
                elif isinstance(child, ArrowNode):
                    pending_arrow = child
                elif isinstance(
                    child,
                    (GroupNode, StackNode, GridNode, ReactionNode, NetworkNode),
                ):
                    walk_container(child)
                elif isinstance(child, TableNode):
                    # Tables are not yet drawn; Markush rtable already on mols.
                    continue
                elif isinstance(child, (ImageNode, TextNode, RefNode, AnnotationNode)):
                    continue
            return

        if isinstance(n, MolNode):
            mid = _ensure_mol_id(n, used_ids, mol_index)
            mol_index += 1
            spec = mol_to_molecule_spec(n)
            if spec.id is None:
                spec = spec.model_copy(update={"id": mid})
            molecules.append(spec)

    if isinstance(
        node, (GroupNode, StackNode, GridNode, ReactionNode, NetworkNode)
    ):
        walk_container(node)
    elif isinstance(node, MolNode):
        walk_container(node)
    else:
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
        return PictSpec.model_validate(
            lift_legacy(spec.model_dump(mode="json"))
        )
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
    from xpict.contracts.shorthand import compress_label

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

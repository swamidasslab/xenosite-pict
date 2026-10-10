"""Syntactic sugar for building live DepictSpec documents (``+`` ``>>`` ``|``).

Not validation — Rust owns that. No imports from ``base`` / host DepictSpec
(avoids import cycles); ``to_spec()`` duck-typing covers ``Mol`` / ``PictBase``.
"""

from __future__ import annotations

from typing import Protocol, runtime_checkable

from xpict.contracts.depict import (
    EdgeNode,
    GridNode,
    GroupNode,
    LayoutOpts,
    MolNode,
    ReactionSchemeNode,
    TextNode,
)
from xpict.contracts.depict import DepictSpec as WireDepictSpec


@runtime_checkable
class _HasToSpec(Protocol):
    def to_spec(self) -> WireDepictSpec: ...


SchemeChild = MolNode | EdgeNode | TextNode
RowRoot = MolNode | GroupNode | ReactionSchemeNode | GridNode
Operand = (
    WireDepictSpec
    | _HasToSpec
    | MolNode
    | EdgeNode
    | TextNode
    | GroupNode
    | ReactionSchemeNode
    | GridNode
)


def _wire(root: RowRoot) -> WireDepictSpec:
    return WireDepictSpec(root)


def mols_of(doc: WireDepictSpec) -> list[MolNode]:
    """Flatten mol nodes in document order (skips edges)."""
    root = doc.root
    if isinstance(root, MolNode):
        return [root]
    if isinstance(root, GridNode):
        out: list[MolNode] = []
        for row in root.children:
            out.extend(mols_of(_wire(_unwrap_row(row))))
        return out
    if isinstance(root, ReactionSchemeNode):
        return [c for c in root.children if isinstance(c, MolNode)]
    return [c for c in root.children if isinstance(c, MolNode)]


def _unwrap_row(row: RowRoot | WireDepictSpec) -> RowRoot:
    if isinstance(row, WireDepictSpec):
        root = row.root
        if isinstance(root, (MolNode, GroupNode, ReactionSchemeNode, GridNode)):
            return root
        raise TypeError(f"grid row cannot be {type(root)!r}")
    return row


def _unique_mol_id(preferred: str, used: set[str], *, counter: list[int]) -> str:
    """Reserve ``preferred`` (or next ``m_<i>``) without clashing with ``used``."""
    if preferred.strip():
        nid = preferred.strip()
        base = nid
        n = 1
        while nid in used:
            nid = f"{base}_{n}"
            n += 1
    else:
        while f"m_{counter[0]}" in used:
            counter[0] += 1
        nid = f"m_{counter[0]}"
        counter[0] += 1
    used.add(nid)
    return nid


def with_mol_ids(nodes: list[MolNode]) -> list[MolNode]:
    used: set[str] = set()
    counter = [0]
    out: list[MolNode] = []
    for node in nodes:
        nid = _unique_mol_id(node.id or "", used, counter=counter)
        out.append(node if node.id == nid else node.model_copy(update={"id": nid}))
    return out


def chain_align_previous(nodes: list[MolNode]) -> list[MolNode]:
    """Stamp unset ``align_to`` to the previous mol id."""
    if not nodes:
        return []
    out = [nodes[0]]
    for i, node in enumerate(nodes[1:], start=1):
        if node.align_to is None:
            node = node.model_copy(update={"align_to": out[i - 1].id})
        out.append(node)
    return out


def as_mol_nodes(other: Operand) -> list[MolNode]:
    if isinstance(other, _HasToSpec):
        return list(mols_of(other.to_spec()))
    if isinstance(other, MolNode):
        return [other]
    if isinstance(other, WireDepictSpec):
        return mols_of(other)
    raise TypeError(f"cannot add {type(other)!r} to DepictSpec")


def as_scheme_children(other: Operand) -> list[SchemeChild]:
    if isinstance(other, _HasToSpec):
        other = other.to_spec()
    if isinstance(other, (MolNode, EdgeNode, TextNode)):
        return [other]
    if isinstance(other, WireDepictSpec):
        root = other.root
        if isinstance(root, MolNode):
            return [root]
        if isinstance(root, GroupNode):
            return [c for c in root.children if isinstance(c, (MolNode, TextNode))]
        if isinstance(root, ReactionSchemeNode):
            return list(root.children)
    raise TypeError(f"cannot join {type(other)!r} with >> / <<")


def _ensure_scheme_ids(children: list[SchemeChild]) -> list[SchemeChild]:
    used: set[str] = set()
    counter = [0]
    out: list[SchemeChild] = []
    for child in children:
        if not isinstance(child, MolNode):
            out.append(child)
            continue
        nid = _unique_mol_id(child.id or "", used, counter=counter)
        out.append(child if child.id == nid else child.model_copy(update={"id": nid}))
    return out


def _mol_run_ids(children: list[SchemeChild], *, from_start: bool) -> list[str]:
    seq = children if from_start else list(reversed(children))
    ids: list[str] = []
    for child in seq:
        if isinstance(child, MolNode):
            ids.append(str(child.id))
        elif isinstance(child, EdgeNode):
            break
    return ids if from_start else list(reversed(ids))


def reaction_join(left: Operand, right: Operand, *, reverse: bool) -> WireDepictSpec:
    left_c = _ensure_scheme_ids(as_scheme_children(left))
    right_c = _ensure_scheme_ids(as_scheme_children(right))
    used = {
        (c.id or "").strip()
        for c in left_c
        if isinstance(c, MolNode) and (c.id or "").strip()
    }
    counter = [0]
    fixed: list[SchemeChild] = []
    for child in right_c:
        if isinstance(child, MolNode):
            nid = _unique_mol_id(child.id or "", used, counter=counter)
            fixed.append(
                child if child.id == nid else child.model_copy(update={"id": nid})
            )
        else:
            fixed.append(child)
    right_c = fixed

    sources = _mol_run_ids(left_c, from_start=False)
    targets = _mol_run_ids(right_c, from_start=True)
    if reverse:
        sources, targets = targets, sources
    if not sources or not targets:
        raise ValueError(">> / << require mol endpoints on both sides")

    src: str | list[str] = sources[0] if len(sources) == 1 else sources
    tgt: str | list[str] = targets[0] if len(targets) == 1 else targets
    return _wire(
        ReactionSchemeNode(
            type="reaction_scheme",
            children=[
                *left_c,
                EdgeNode(type="edge", sources=src, targets=tgt, arrow="forward"),
                *right_c,
            ],
            layout=LayoutOpts(direction="right"),
        )
    )


def as_grid_rows(other: Operand) -> list[RowRoot]:
    if isinstance(other, _HasToSpec):
        other = other.to_spec()
    if isinstance(other, WireDepictSpec):
        root = other.root
        if isinstance(root, GridNode):
            return [_unwrap_row(c) for c in root.children]
        if isinstance(root, (MolNode, GroupNode, ReactionSchemeNode, GridNode)):
            return [root]
    if isinstance(other, (MolNode, GroupNode, ReactionSchemeNode, GridNode)):
        return [other]
    raise TypeError(f"cannot join {type(other)!r} with |")


def _remap_ids(raw: str | list[str], remap: dict[str, str]) -> str | list[str]:
    if isinstance(raw, list):
        return [remap.get(str(x), str(x)) for x in raw]
    return remap.get(str(raw), str(raw))


def uniquify_grid_rows(rows: list[RowRoot]) -> list[RowRoot]:
    used: set[str] = set()
    out: list[RowRoot] = []
    for row in rows:
        row = _unwrap_row(row)
        if isinstance(row, GridNode):
            nested = uniquify_grid_rows(list(row.children))
            out.append(row.model_copy(update={"children": nested}))
            for r in nested:
                for n in mols_of(_wire(r)):
                    if (n.id or "").strip():
                        used.add((n.id or "").strip())
            continue

        nodes = (
            [row]
            if isinstance(row, MolNode)
            else [c for c in row.children if isinstance(c, MolNode)]
        )
        remap: dict[str, str] = {}
        new_nodes: list[MolNode] = []
        counter = [0]
        for node in nodes:
            old = (node.id or "").strip()
            nid = _unique_mol_id(old, used, counter=counter)
            if old and old != nid:
                remap[old] = nid
            if node.id != nid:
                node = node.model_copy(update={"id": nid})
            ref = node.align_to
            if isinstance(ref, str) and ref in remap:
                node = node.model_copy(update={"align_to": remap[ref]})
            new_nodes.append(node)

        if isinstance(row, MolNode):
            out.append(new_nodes[0])
            continue

        mol_iter = iter(new_nodes)
        children: list[SchemeChild] = []
        for child in row.children:
            if isinstance(child, MolNode):
                children.append(next(mol_iter))
            elif isinstance(child, EdgeNode) and remap:
                children.append(
                    child.model_copy(
                        update={
                            "sources": _remap_ids(child.sources, remap),
                            "targets": _remap_ids(child.targets, remap),
                        }
                    )
                )
            else:
                children.append(child)
        out.append(row.model_copy(update={"children": children}))
    return out


def grid_join(left: Operand, right: Operand) -> WireDepictSpec:
    rows = uniquify_grid_rows([*as_grid_rows(left), *as_grid_rows(right)])
    return _wire(GridNode(type="grid", children=list(rows)))


def group_add(left: Operand, right: Operand) -> WireDepictSpec:
    children = chain_align_previous(
        with_mol_ids([*as_mol_nodes(left), *as_mol_nodes(right)])
    )
    return _wire(GroupNode(type="group", align=False, children=list(children)))

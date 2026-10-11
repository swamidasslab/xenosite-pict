"""Declarative DepictSpec — thin adapter over Rust ``depict`` + ``compose_scheme``."""

from __future__ import annotations

from collections.abc import Iterator

from xpict.contracts import DepictSpecDict, DocPaintDict, RenderedDict, SceneDict
from xpict.contracts.depict import (
    GridNode,
    GroupNode,
    MolNode,
    ReactionSchemeNode,
)
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.edge import MoleculeIn
from xpict.contracts.scene import Scene
from xpict.native_bridge import DepictSpecLike, compose_scheme, depict as _native_depict
from xpict.rendered import Rendered, SvgAtom, SvgBond, scene_to_svg
from xpict.sugar import mols_of

# Mol node ↔ painted row (document order; ids may be assigned only on the paint side).
PaintPair = tuple[MolNode, RenderedDict]


def as_depict_spec(spec: DepictSpecLike) -> WireDepictSpec:
    """Pydantic-validate a live DepictSpec (model, dict, or JSON string)."""
    if isinstance(spec, str):
        return WireDepictSpec.model_validate_json(spec)
    return WireDepictSpec.model_validate(spec)


def as_rendered(row: RenderedDict, *, scene: SceneDict | None = None) -> Rendered:
    """Bind a Rust ``Rendered`` JSON object — packing already happened in core."""
    sc: SceneDict = scene if scene is not None else row["scene"]
    return Rendered(
        width=float(sc.get("width", row["width"])),
        height=float(sc.get("height", row["height"])),
        scene=Scene.model_validate(sc),
        molecule=MoleculeIn.model_validate(row["molecule"]),
        source=str(row["source"]),
        frame_molblock=str(row.get("frame_molblock") or ""),
        coords=[SvgAtom(**a) for a in row["coords"]],
        svg_coords=[SvgAtom(**a) for a in row["svg_coords"]],
        bonds=[SvgBond(**b) for b in row["bonds"]],
    )


def pair_paints(doc: WireDepictSpec, painted: list[RenderedDict]) -> list[PaintPair]:
    """Zip document mols to Rust paint rows (same order as ``assign_mol_ids``)."""
    mols = mols_of(doc)
    if len(mols) != len(painted):
        raise RuntimeError(
            f"depict paint count {len(painted)} != mol count {len(mols)}"
        )
    return list(zip(mols, painted, strict=True))


def _paint_of(pairs: list[PaintPair], node: MolNode) -> RenderedDict | None:
    for m, row in pairs:
        if m is node:
            return row
    mid = (node.id or "").strip()
    if not mid:
        return None
    for m, row in pairs:
        if (m.id or "").strip() == mid:
            return row
    return None


def _compose_scheme_cell(scheme: ReactionSchemeNode, pairs: list[PaintPair]) -> Rendered:
    """Rust ``compose_scheme`` for one scheme subtree + its painted mols."""
    paints: list[DocPaintDict] = []
    first: RenderedDict | None = None
    for m in mols_of(WireDepictSpec(scheme)):
        row = _paint_of(pairs, m)
        if row is None:
            continue
        if first is None:
            first = row
        paints.append(
            {"id": row["id"], "molecule": row["molecule"], "scene": row["scene"]}
        )
    if not paints or first is None:
        raise RuntimeError("compose_scheme: no painted mols for reaction_scheme")
    composed = compose_scheme(WireDepictSpec(scheme), paints)
    return as_rendered({**first, "source": "reaction_scheme"}, scene=composed)


def iter_mol_cells(
    root: MolNode | GroupNode | ReactionSchemeNode | GridNode,
    pairs: list[PaintPair],
) -> Iterator[tuple[MolNode | None, Rendered]]:
    """Walk the spec tree with mol nodes (``None`` for composed scheme cells)."""
    if isinstance(root, ReactionSchemeNode):
        yield None, _compose_scheme_cell(root, pairs)
        return
    if isinstance(root, GridNode):
        for row in root.children:
            yield from iter_mol_cells(row, pairs)
        return
    if isinstance(root, MolNode):
        row = _paint_of(pairs, root)
        if row is not None:
            yield root, as_rendered(row)
        return
    for child in root.children:
        if isinstance(child, MolNode):
            row = _paint_of(pairs, child)
            if row is not None:
                yield child, as_rendered(row)


def iter_cells(
    root: MolNode | GroupNode | ReactionSchemeNode | GridNode,
    pairs: list[PaintPair],
) -> Iterator[Rendered]:
    """Walk the spec tree: mol/group → painted rows; scheme → one composed scene."""
    for _node, painted in iter_mol_cells(root, pairs):
        yield painted


def depict(spec: DepictSpecLike) -> list[Rendered]:
    """One Rust ``depict``, then package cells (nested schemes compose)."""
    doc = as_depict_spec(spec)
    painted = _native_depict(doc)
    if not painted:
        return []
    return list(iter_cells(doc.root, pair_paints(doc, painted)))


def depict_svg(spec: DepictSpecLike) -> str:
    """Document depict then SVG (composed scene per scheme cell)."""
    rows = depict(spec)
    if not rows:
        return ""
    if len(rows) == 1:
        return scene_to_svg(rows[0].scene)
    return "\n".join(scene_to_svg(r.scene) for r in rows)


def is_live_depict_spec(obj: WireDepictSpec | DepictSpecDict) -> bool:
    """True for nested live DepictSpec (``mol|group|reaction_scheme|grid``)."""
    if isinstance(obj, WireDepictSpec):
        return True
    return obj.get("type") in ("mol", "group", "reaction_scheme", "grid")

"""Declarative DepictSpec — thin adapter over Rust ``depict``."""

from __future__ import annotations

from xpict.contracts import DepictSpecDict, DocPaintDict, RenderedDict, SceneDict
from xpict.contracts.depict import ReactionSchemeNode
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.edge import MoleculeIn
from xpict.contracts.scene import Scene
from xpict.draw.svg import scene_to_svg
from xpict.native_bridge import compose_scheme, depict as _native_depict
from xpict.rendered import Rendered, SvgAtom, SvgBond

DepictSpecLike = WireDepictSpec | DepictSpecDict | str


def as_depict_spec(spec: DepictSpecLike) -> WireDepictSpec:
    """Pydantic-validate a live DepictSpec (model, dict, or JSON string)."""
    if isinstance(spec, str):
        return WireDepictSpec.model_validate_json(spec)
    return WireDepictSpec.model_validate(spec)


def _is_scheme(doc: WireDepictSpec) -> bool:
    root = doc.root
    return isinstance(root, ReactionSchemeNode) or getattr(root, "type", None) == "reaction_scheme"


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


def depict(spec: DepictSpecLike) -> list[Rendered]:
    """Declarative document → ``list[Rendered]`` via one Rust ``depict`` call."""
    doc = as_depict_spec(spec)
    painted = _native_depict(doc)
    if not painted:
        return []

    if _is_scheme(doc):
        paints: list[DocPaintDict] = [
            {"id": r["id"], "molecule": r["molecule"], "scene": r["scene"]}
            for r in painted
        ]
        composed = compose_scheme(doc, paints)
        first: RenderedDict = {**painted[0], "source": "reaction_scheme"}
        return [as_rendered(first, scene=composed)]

    return [as_rendered(row) for row in painted]


def depict_svg(spec: DepictSpecLike) -> str:
    """Document depict then SVG (one composed scene for schemes)."""
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
    t = obj.get("type") if isinstance(obj, dict) else None
    return t in ("mol", "group", "reaction_scheme", "grid")

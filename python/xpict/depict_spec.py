"""Declarative DepictSpec — thin adapter over Rust ``render``."""

from __future__ import annotations

from xpict.contracts import DepictSpecDict, RenderedDict, SceneDict
from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.edge import MoleculeIn
from xpict.contracts.scene import Scene
from xpict.native_bridge import DepictSpecLike, depict as _native_depict
from xpict.rendered import Rendered, SvgAtom, SvgBond, scene_to_svg


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


def depict(spec: DepictSpecLike) -> list[Rendered]:
    """One Rust ``render``: layout, mol-spec paint, pack cells."""
    doc = as_depict_spec(spec)
    painted = _native_depict(doc)
    return [as_rendered(row) for row in painted]


def is_live_depict_spec(obj: WireDepictSpec | DepictSpecDict) -> bool:
    """True for nested live DepictSpec (``mol|group|reaction_scheme|grid``)."""
    if isinstance(obj, WireDepictSpec):
        return True
    return obj.get("type") in ("mol", "group", "reaction_scheme", "grid")


def depict_svg(spec: DepictSpecLike) -> str:
    """Document render then SVG (composed scene per scheme cell)."""
    rows = depict(spec)
    if not rows:
        return ""
    if len(rows) == 1:
        return scene_to_svg(rows[0].scene)
    return "\n".join(scene_to_svg(r.scene) for r in rows)

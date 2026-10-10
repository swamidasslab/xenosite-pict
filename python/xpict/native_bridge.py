"""Rust core bindings (``xpict._native``). Extension is required on the ship path."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import TYPE_CHECKING, cast

from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.depict_dict import DepictSpecDict, MolOptsPatchDict
from xpict.contracts.edge import EdgePlan, EdgeResult
from xpict.contracts.edge_dict import EdgePlanDict, EdgeResultDict
from xpict.contracts.rendered_dict import DocPaintDict, RenderedDict
from xpict.contracts.scene_dict import SceneDict

try:
    from xpict import _native as _rust
except ImportError as e:  # pragma: no cover
    raise ImportError(
        "xpict requires the Rust extension (xpict._native). "
        "Build with: maturin develop --manifest-path crates/xpict-py/Cargo.toml"
    ) from e

HAS_RUST_CORE: bool = True

if TYPE_CHECKING:
    from xpict._native import Shape as Shape
else:
    Shape = _rust.Shape

DepictSpecLike = WireDepictSpec | DepictSpecDict | str
EdgePlanLike = EdgePlan | EdgePlanDict | str
EdgeResultLike = EdgeResult | EdgeResultDict | str


@dataclass(frozen=True, slots=True)
class CapsuleInk:
    """Tagged ink for a line capsule — halo via Rust when available."""

    x1: float
    y1: float
    x2: float
    y2: float
    radius: float


@dataclass(frozen=True, slots=True)
class DiskInk:
    """Tagged ink for a filled disk."""

    cx: float
    cy: float
    radius: float


InkGeometry = Shape | CapsuleInk | DiskInk


def multi_bond_offset(length: float) -> float:
    return float(_rust.multi_bond_offset(length))


def capsule_halo_path_d(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    ink_radius: float,
    grow: float,
) -> str | None:
    return _rust.capsule_halo_path_d(x1, y1, x2, y2, ink_radius, grow)


def disk_halo_path_d(cx: float, cy: float, ink_radius: float, grow: float) -> str | None:
    return _rust.disk_halo_path_d(cx, cy, ink_radius, grow)


def elk_layout_json(graph_json: str) -> str | None:
    """Native elkrs layout."""
    fn = getattr(_rust, "elk_layout_json", None)
    if fn is None:
        return None
    return str(fn(graph_json))


def _as_json(obj: DepictSpecLike | EdgePlanLike | EdgeResultLike) -> str:
    if isinstance(obj, str):
        return obj
    dump = getattr(obj, "model_dump", None)
    if callable(dump):
        return json.dumps(dump(mode="json"))
    return json.dumps(obj)


def validate_edge_plan(plan: EdgePlanLike) -> EdgePlanDict:
    """Structural checks via Rust core."""
    return cast(EdgePlanDict, json.loads(_rust.validate_edge_plan(_as_json(plan))))


def plan_edge(spec: DepictSpecLike) -> EdgePlanDict | None:
    """Pass 1: DepictSpec → EdgePlan (Rust core). ``None`` when empty."""
    raw = _rust.plan_edge(_as_json(spec))
    return cast(EdgePlanDict | None, json.loads(raw))


def resolve_depict_chrome(spec: DepictSpecLike) -> list[MolOptsPatchDict]:
    """Resolve cascading mol opts in document order (Rust DepictSpec semantics)."""
    return cast(
        list[MolOptsPatchDict],
        json.loads(_rust.resolve_depict_chrome(_as_json(spec))),
    )


def render_doc(spec: DepictSpecLike, edge: EdgeResultLike) -> list[RenderedDict]:
    """Pass 2: DepictSpec + EdgeResult → DocPaint rows (Rust core).

    Prefer :func:`depict` for the product path (plan + edge + paint in one call).
    """
    return cast(
        list[RenderedDict],
        json.loads(_rust.render_doc(_as_json(spec), _as_json(edge))),
    )


def depict(spec: DepictSpecLike) -> list[RenderedDict]:
    """DepictSpec → ``Rendered`` rows (plan + Chematic/CoordGen edge + paint in Rust)."""
    fn = getattr(_rust, "depict", None)
    if fn is None:
        raise RuntimeError(
            "xpict._native.depict requires CoordGen (rebuild with coordgen feature)"
        )
    return cast(list[RenderedDict], json.loads(fn(_as_json(spec))))


def compose_scheme(
    spec: DepictSpecLike, paints: list[DocPaintDict] | str
) -> SceneDict:
    """Pass 3: DepictSpec + DocPaint rows → composed Scene (ELK + overlays)."""
    paints_json = paints if isinstance(paints, str) else json.dumps(paints)
    return cast(SceneDict, json.loads(_rust.compose_scheme(_as_json(spec), paints_json)))


def halo_path_d_for_ink(ink: InkGeometry, dist: float) -> str | None:
    """Rust fast path for tagged ink; ``None`` → caller uses Shape.halo."""
    if isinstance(ink, CapsuleInk):
        return capsule_halo_path_d(
            ink.x1, ink.y1, ink.x2, ink.y2, ink.radius, dist
        )
    if isinstance(ink, DiskInk):
        return disk_halo_path_d(ink.cx, ink.cy, ink.radius, dist)
    return None

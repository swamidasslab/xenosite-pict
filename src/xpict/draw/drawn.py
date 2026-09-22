"""Unified drawable result: primitives + ink + occupancy boxes."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from xpict.contracts.scene import (
    CirclePrim,
    Layer,
    LayerName,
    PathPrim,
    Primitive,
    TextPrim,
)
from xpict.draw.halo import halo_from_shapes
from xpict.draw.metrics import HALO_GAP_PX, HALO_OPACITY
from xpict.draw.paths import shift_path_d
from xpict.native_bridge import CapsuleInk, DiskInk, Shape

Box = tuple[float, float, float, float]
HaloJob = tuple[Any, float]  # (ink, dist)


@dataclass
class Drawn:
    """One drawable's contribution to the scene.

    ``ink`` feeds the bottom halo layer; ``boxes`` stamp the collision grid.
    """

    primitives: list[Primitive] = field(default_factory=list)
    ink: list[Any] = field(default_factory=list)
    boxes: list[Box] = field(default_factory=list)
    layer: LayerName = "marks"
    halo_cls: str = "halo"
    # Parallel to ``ink``; ``None`` → ``HALO_GAP_PX``. Empty → all default.
    ink_dists: list[float | None] = field(default_factory=list)

    def extend(self, other: Drawn | None) -> Drawn:
        if other is None:
            return self
        self.primitives.extend(other.primitives)
        self.ink.extend(other.ink)
        self.boxes.extend(other.boxes)
        self.ink_dists.extend(other.ink_dists)
        return self


def _as_shape(ink: Any) -> Shape | None:
    if ink is None:
        return None
    if isinstance(ink, CapsuleInk):
        return Shape.capsule(ink.x1, ink.y1, ink.x2, ink.y2, ink.radius)
    if isinstance(ink, DiskInk):
        return Shape.disk(ink.cx, ink.cy, ink.radius)
    if getattr(ink, "is_empty", False):
        return None
    return ink


def shift_ink(ink: Any, dx: float, dy: float) -> Any:
    """Translate tagged ink or Shape by ``(dx, dy)``."""
    if abs(dx) < 1e-12 and abs(dy) < 1e-12:
        return ink
    if isinstance(ink, CapsuleInk):
        return CapsuleInk(ink.x1 + dx, ink.y1 + dy, ink.x2 + dx, ink.y2 + dy, ink.radius)
    if isinstance(ink, DiskInk):
        return DiskInk(ink.cx + dx, ink.cy + dy, ink.radius)
    return ink.translate(dx, dy)


def drawn_halo_jobs(drawn: Drawn) -> list[HaloJob]:
    """Ink geometries and buffer distances from one :class:`Drawn`."""
    jobs: list[HaloJob] = []
    for i, geom in enumerate(drawn.ink):
        if geom is None:
            continue
        dist = (
            drawn.ink_dists[i]
            if i < len(drawn.ink_dists) and drawn.ink_dists
            else None
        )
        jobs.append((geom, HALO_GAP_PX if dist is None else dist))
    return jobs


def union_halo_prim(
    jobs: list[HaloJob],
    *,
    cls: str = "halo",
) -> PathPrim | None:
    """One soft white knockout for all ink (union of per-ink halos)."""
    acc: Shape | None = None
    for ink, dist in jobs:
        grown = halo_from_shapes(_as_shape(ink), dist)
        if grown is None:
            continue
        acc = grown if acc is None else acc.union(grown)
    if acc is None or acc.is_empty:
        return None
    d = acc.to_svg_d()
    if not d:
        return None
    return PathPrim(
        d=d,
        stroke="none",
        fill="#fff",
        stroke_width=0.0,
        opacity=HALO_OPACITY,
        cls=cls,
    )


def halo_prims(
    ink: Any,
    dist: float | None = None,
    *,
    cls: str = "halo",
) -> list[PathPrim]:
    """Knockout PathPrims for one ink geometry."""
    prim = union_halo_prim([(ink, HALO_GAP_PX if dist is None else dist)], cls=cls)
    return [prim] if prim is not None else []


def emit_drawn(
    layers: dict[str, Layer],
    drawn: Drawn,
    *,
    halo: bool = True,
    halo_jobs: list[HaloJob] | None = None,
) -> None:
    """Append primitives to ``drawn.layer``; collect ink for a later union halo."""
    layers[drawn.layer].primitives.extend(drawn.primitives)
    if not halo:
        return
    jobs = drawn_halo_jobs(drawn)
    if halo_jobs is not None:
        halo_jobs.extend(jobs)
        return
    # Fallback: emit a union for this Drawn alone (no collector).
    prim = union_halo_prim(jobs, cls="halo")
    if prim is not None:
        layers["halo"].primitives.append(prim)


def shift_layers(layers: dict[str, Layer], dx: float, dy: float) -> None:
    """Translate every primitive in ``layers`` by ``(dx, dy)``."""
    if abs(dx) < 1e-12 and abs(dy) < 1e-12:
        return
    for layer in layers.values():
        shifted: list[Primitive] = []
        for p in layer.primitives:
            if isinstance(p, PathPrim):
                shifted.append(p.model_copy(update={"d": shift_path_d(p.d, dx, dy)}))
            elif isinstance(p, CirclePrim):
                shifted.append(p.model_copy(update={"cx": p.cx + dx, "cy": p.cy + dy}))
            elif isinstance(p, TextPrim):
                shifted.append(p.model_copy(update={"x": p.x + dx, "y": p.y + dy}))
            else:
                shifted.append(p)
        layer.primitives = shifted


__all__ = [
    "Box",
    "Drawn",
    "HaloJob",
    "drawn_halo_jobs",
    "emit_drawn",
    "halo_prims",
    "shift_ink",
    "shift_layers",
    "union_halo_prim",
]

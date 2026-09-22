"""Unified drawable result: primitives + ink + occupancy boxes."""

from __future__ import annotations

from collections.abc import Sequence
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

    Set ``halo=True`` to opt this drawable's ``ink`` into the shared
    :class:`Halo` (backbone, element symbols, annotations). Shading,
    captions, marks, and diagram overlays leave it ``False``.
    """

    primitives: list[Primitive] = field(default_factory=list)
    ink: list[Any] = field(default_factory=list)
    boxes: list[Box] = field(default_factory=list)
    layer: LayerName = "marks"
    halo: bool = False
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
        self.halo = self.halo or other.halo
        return self


@dataclass
class Halo:
    """Single document-space knockout. Drawables opt in via :meth:`add`."""

    jobs: list[HaloJob] = field(default_factory=list)

    def add(self, ink: Any, dist: float | None = None) -> None:
        if ink is None:
            return
        self.jobs.append((ink, HALO_GAP_PX if dist is None else dist))

    def extend(self, jobs: Sequence[HaloJob]) -> None:
        self.jobs.extend(jobs)

    def shift(self, dx: float, dy: float) -> None:
        if abs(dx) < 1e-12 and abs(dy) < 1e-12:
            return
        self.jobs = [(shift_ink(ink, dx, dy), dist) for ink, dist in self.jobs]

    def to_prim(self, *, cls: str = "halo") -> PathPrim | None:
        return union_halo_prim(self.jobs, cls=cls)

    def __bool__(self) -> bool:
        return bool(self.jobs)


def _as_shape(ink: Any) -> Shape | None:
    match ink:
        case None:
            return None
        case CapsuleInk():
            return Shape.capsule(ink.x1, ink.y1, ink.x2, ink.y2, ink.radius)
        case DiskInk():
            return Shape.disk(ink.cx, ink.cy, ink.radius)
        case _ if getattr(ink, "is_empty", False):
            return None
        case _:
            return ink


def shift_ink(ink: Any, dx: float, dy: float) -> Any:
    """Translate tagged ink or Shape by ``(dx, dy)``."""
    if abs(dx) < 1e-12 and abs(dy) < 1e-12:
        return ink
    match ink:
        case CapsuleInk():
            return CapsuleInk(ink.x1 + dx, ink.y1 + dy, ink.x2 + dx, ink.y2 + dy, ink.radius)
        case DiskInk():
            return DiskInk(ink.cx + dx, ink.cy + dy, ink.radius)
        case _:
            return ink.translate(dx, dy)


def drawn_halo_jobs(drawn: Drawn) -> list[HaloJob]:
    """Ink geometries and buffer distances from one opting-in :class:`Drawn`."""
    if not drawn.halo:
        return []
    jobs: list[HaloJob] = []
    for i, geom in enumerate(drawn.ink):
        if geom is None:
            continue
        dist = drawn.ink_dists[i] if i < len(drawn.ink_dists) and drawn.ink_dists else None
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
    halo: Halo | None = None,
    halo_enabled: bool = True,
) -> None:
    """Append primitives to ``drawn.layer``; opt into ``halo`` when requested."""
    layers[drawn.layer].primitives.extend(drawn.primitives)
    if not halo_enabled or not drawn.halo:
        return
    jobs = drawn_halo_jobs(drawn)
    if not jobs:
        return
    if halo is not None:
        halo.extend(jobs)
        return
    # Fallback: emit a union for this Drawn alone (no shared collector).
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
            match p:
                case PathPrim():
                    shifted.append(p.model_copy(update={"d": shift_path_d(p.d, dx, dy)}))
                case CirclePrim():
                    shifted.append(p.model_copy(update={"cx": p.cx + dx, "cy": p.cy + dy}))
                case TextPrim():
                    shifted.append(p.model_copy(update={"x": p.x + dx, "y": p.y + dy}))
                case _:
                    shifted.append(p)
        layer.primitives = shifted


__all__ = [
    "Box",
    "Drawn",
    "Halo",
    "HaloJob",
    "drawn_halo_jobs",
    "emit_drawn",
    "halo_prims",
    "shift_ink",
    "shift_layers",
    "union_halo_prim",
]

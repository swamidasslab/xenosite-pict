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
from xpict.draw.halo import halo_path_d
from xpict.draw.metrics import LABEL_GAP_PX
from xpict.draw.paths import shift_path_d

Box = tuple[float, float, float, float]


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
    # Parallel to ``ink``; ``None`` → ``LABEL_GAP_PX``. Empty → all default.
    ink_dists: list[float | None] = field(default_factory=list)

    def extend(self, other: Drawn | None) -> Drawn:
        if other is None:
            return self
        self.primitives.extend(other.primitives)
        self.ink.extend(other.ink)
        self.boxes.extend(other.boxes)
        self.ink_dists.extend(other.ink_dists)
        return self


def halo_prims(
    ink: Any,
    dist: float | None = None,
    *,
    cls: str = "halo",
) -> list[PathPrim]:
    """White knockout PathPrims for one ink geometry (bottom layer only)."""
    d = halo_path_d(ink, dist)
    if not d:
        return []
    return [
        PathPrim(
            d=d,
            stroke="none",
            fill="#fff",
            stroke_width=0.0,
            opacity=1.0,
            cls=cls,
        )
    ]


def emit_drawn(
    layers: dict[str, Layer],
    drawn: Drawn,
    *,
    halo: bool = True,
) -> None:
    """Append primitives to ``drawn.layer`` and optional halo knockouts."""
    layers[drawn.layer].primitives.extend(drawn.primitives)
    if not halo:
        return
    for i, geom in enumerate(drawn.ink):
        if geom is None:
            continue
        dist = (
            drawn.ink_dists[i]
            if i < len(drawn.ink_dists) and drawn.ink_dists
            else None
        )
        if dist is None:
            dist = LABEL_GAP_PX
        layers["halo"].primitives.extend(
            halo_prims(geom, dist, cls=drawn.halo_cls)
        )


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
    "emit_drawn",
    "halo_prims",
    "shift_layers",
]

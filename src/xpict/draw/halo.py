"""Halo knockouts from ink shapes.

One entry point — :func:`halo_from_shapes` — grows a white knockout around any
ink geometry (glyph fills, bond capsules, mark rings, …). Counters in letters
like ``O`` / ``A`` stay open so the halo does not fill those circles.
"""

from __future__ import annotations

from xpict.draw.metrics import LABEL_GAP_PX
from xpict.native_bridge import CapsuleInk, DiskInk, Shape, halo_path_d_for_ink


def halo_from_shapes(
    ink: Shape | None,
    dist: float | None = None,
) -> Shape | None:
    """Grow a knockout around ``ink`` shapes.

    ``dist`` defaults to ``LABEL_GAP_PX``. Interior counters (the hole in
    ``O``, ring marks, …) are punched back out so the halo stays hollow there.
    """
    if ink is None or ink.is_empty:
        return None
    radius = LABEL_GAP_PX if dist is None else dist
    if radius <= 0:
        return None
    grown = ink.halo(radius)
    if grown.is_empty:
        return None
    return grown


def halo_path_d(
    ink: Shape | CapsuleInk | DiskInk | None,
    dist: float | None = None,
) -> str | None:
    """SVG path ``d`` for :func:`halo_from_shapes`."""
    radius = LABEL_GAP_PX if dist is None else dist
    if ink is not None and radius > 0:
        fast = halo_path_d_for_ink(ink, radius)
        if fast:
            return fast
    if isinstance(ink, (CapsuleInk, DiskInk)):
        # Tagged ink without Rust fast path — expand to Shape.
        if isinstance(ink, CapsuleInk):
            shape = Shape.capsule(ink.x1, ink.y1, ink.x2, ink.y2, ink.radius)
        else:
            shape = Shape.disk(ink.cx, ink.cy, ink.radius)
        halo = halo_from_shapes(shape, dist)
    else:
        halo = halo_from_shapes(ink, dist)
    if halo is None or halo.is_empty:
        return None
    d = halo.to_svg_d()
    return d or None


def capsule_shape(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    radius: float,
) -> Shape | CapsuleInk | None:
    """Filled capsule (segment thickened by ``radius``) as ink geometry."""
    if radius <= 0:
        return None
    return CapsuleInk(x1, y1, x2, y2, radius)


def circle_ring_shape(
    cx: float,
    cy: float,
    r: float,
    stroke_width: float,
) -> Shape | None:
    """Annular ink for a stroked circle (interior stays hollow)."""
    if r <= 0 or stroke_width <= 0:
        return None
    shape = Shape.annular(cx, cy, r, stroke_width)
    return None if shape.is_empty else shape


def disk_shape(cx: float, cy: float, r: float) -> Shape | DiskInk | None:
    if r <= 0:
        return None
    return DiskInk(cx, cy, r)


def path_polyline_shape(
    coords: list[tuple[float, float]],
    radius: float,
) -> Shape | CapsuleInk | None:
    """Capsule along a polyline (bond shafts, mark connectors)."""
    if len(coords) < 2 or radius <= 0:
        return None
    if len(coords) == 2:
        return capsule_shape(coords[0][0], coords[0][1], coords[1][0], coords[1][1], radius)
    shape = Shape.polyline_buffer(coords, radius)
    return None if shape.is_empty else shape


def union_shapes(*geoms: Shape | CapsuleInk | DiskInk | None) -> Shape | None:
    parts: list[Shape] = []
    for g in geoms:
        if g is None:
            continue
        if isinstance(g, CapsuleInk):
            s = Shape.capsule(g.x1, g.y1, g.x2, g.y2, g.radius)
        elif isinstance(g, DiskInk):
            s = Shape.disk(g.cx, g.cy, g.radius)
        else:
            s = g
        if not s.is_empty:
            parts.append(s)
    if not parts:
        return None
    acc = parts[0]
    for p in parts[1:]:
        acc = acc.union(p)
    return None if acc.is_empty else acc


__all__ = [
    "halo_from_shapes",
    "halo_path_d",
    "capsule_shape",
    "circle_ring_shape",
    "disk_shape",
    "path_polyline_shape",
    "union_shapes",
]

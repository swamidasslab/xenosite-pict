"""Halo knockouts from ink shapes.

One entry point — :func:`halo_from_shapes` — grows a white knockout around any
ink geometry (glyph fills, bond capsules, mark rings, …). Counters in letters
like ``O`` / ``A`` stay open so the halo does not fill those circles.
"""

from __future__ import annotations

from shapely.geometry import LineString, MultiPolygon, Point, Polygon
from shapely.geometry.base import BaseGeometry
from shapely.ops import unary_union

from xpict.draw.glyphs import geom_to_svg_d
from xpict.draw.metrics import LABEL_GAP_PX


def _polygons(geom: BaseGeometry) -> list[Polygon]:
    if geom.is_empty:
        return []
    if isinstance(geom, Polygon):
        return [geom]
    if isinstance(geom, MultiPolygon):
        return list(geom.geoms)
    return [g for g in getattr(geom, "geoms", []) if isinstance(g, Polygon)]


def _counters(geom: BaseGeometry) -> BaseGeometry | None:
    """Union of interior rings (glyph counters, hollow circles)."""
    holes: list[Polygon] = []
    for poly in _polygons(geom):
        for ring in poly.interiors:
            hole = Polygon(ring)
            if not hole.is_empty:
                holes.append(hole)
    if not holes:
        return None
    return unary_union(holes)


def halo_from_shapes(
    ink: BaseGeometry | None,
    dist: float | None = None,
) -> BaseGeometry | None:
    """Grow a knockout around ``ink`` shapes.

    ``dist`` defaults to ``LABEL_GAP_PX``. Interior counters (the hole in
    ``O``, ring marks, …) are punched back out so the halo stays hollow there.
    """
    if ink is None or ink.is_empty:
        return None
    radius = LABEL_GAP_PX if dist is None else dist
    if radius <= 0:
        return None
    grown = ink.buffer(radius, quad_segs=8)
    if grown.is_empty:
        return None
    counters = _counters(ink)
    if counters is not None and not counters.is_empty:
        grown = grown.difference(counters)
    if grown.is_empty:
        return None
    return grown


def halo_path_d(
    ink: BaseGeometry | None,
    dist: float | None = None,
) -> str | None:
    """SVG path ``d`` for :func:`halo_from_shapes`."""
    halo = halo_from_shapes(ink, dist)
    if halo is None or halo.is_empty:
        return None
    d = geom_to_svg_d(halo)
    return d or None


def capsule_shape(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    radius: float,
) -> BaseGeometry | None:
    """Filled capsule (segment thickened by ``radius``) as ink geometry."""
    if radius <= 0:
        return None
    if abs(x2 - x1) < 1e-12 and abs(y2 - y1) < 1e-12:
        return Point(x1, y1).buffer(radius, quad_segs=8)
    return LineString([(x1, y1), (x2, y2)]).buffer(radius, quad_segs=8, cap_style=1)


def circle_ring_shape(
    cx: float,
    cy: float,
    r: float,
    stroke_width: float,
) -> BaseGeometry | None:
    """Annular ink for a stroked circle (interior stays hollow)."""
    if r <= 0 or stroke_width <= 0:
        return None
    outer = Point(cx, cy).buffer(r + 0.5 * stroke_width, quad_segs=16)
    inner_r = r - 0.5 * stroke_width
    if inner_r > 1e-6:
        inner = Point(cx, cy).buffer(inner_r, quad_segs=16)
        return outer.difference(inner)
    return outer


def disk_shape(cx: float, cy: float, r: float) -> BaseGeometry | None:
    if r <= 0:
        return None
    return Point(cx, cy).buffer(r, quad_segs=12)


def path_polyline_shape(
    coords: list[tuple[float, float]],
    radius: float,
) -> BaseGeometry | None:
    """Capsule along a polyline (bond shafts, mark connectors)."""
    if len(coords) < 2 or radius <= 0:
        return None
    if len(coords) == 2:
        return capsule_shape(coords[0][0], coords[0][1], coords[1][0], coords[1][1], radius)
    line = LineString(coords)
    if line.is_empty:
        return None
    return line.buffer(radius, quad_segs=8, cap_style=1, join_style=1)


def union_shapes(*geoms: BaseGeometry | None) -> BaseGeometry | None:
    parts = [g for g in geoms if g is not None and not g.is_empty]
    if not parts:
        return None
    if len(parts) == 1:
        return parts[0]
    return unary_union(parts)


__all__ = [
    "halo_from_shapes",
    "halo_path_d",
    "capsule_shape",
    "circle_ring_shape",
    "disk_shape",
    "path_polyline_shape",
    "union_shapes",
]

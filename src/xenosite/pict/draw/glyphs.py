"""Label glyph outlines from the bundled Liberation Sans face.

Atom labels stay as SVG ``<text>`` for the ink. The white knockout under
them is a **shapely buffer** of the same glyph outlines, serialized as a
path so the halo does not depend on the viewer having the font.
"""

from __future__ import annotations

from fontTools.pens.transformPen import TransformPen
from shapely import affinity
from shapely.geometry import MultiPolygon, Polygon
from shapely.geometry.base import BaseGeometry
from shapely.ops import unary_union

# Re-export for callers that imported from here.
from xenosite.pict.draw.font_face import (
    ContourPen,
    bundled_font_path,  # noqa: F401
    glyph_set_cmap_upem,
)
from xenosite.pict.draw.metrics import FONT_PX, HALO_STROKE
from xenosite.pict.draw.text_metrics import measure_text


def label_outline(
    text: str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
) -> BaseGeometry | None:
    """Glyph fill geometry in SVG coords for ``text`` at baseline ``(x, y)``."""
    if not text:
        return None
    glyph_set, cmap, upem = glyph_set_cmap_upem()
    pen = ContourPen(glyph_set)
    pen_x = 0.0
    for ch in text:
        name = cmap.get(ord(ch))
        if name is None:
            pen_x += 0.5 * upem
            continue
        glyph = glyph_set[name]  # type: ignore[index]
        tpen = TransformPen(pen, (1.0, 0.0, 0.0, 1.0, pen_x, 0.0))
        glyph.draw(tpen)
        pen_x += float(glyph.width)

    polys: list[Polygon] = []
    for contour in pen.contours:
        if len(contour) < 3:
            continue
        poly = Polygon(contour)
        if poly.is_empty:
            continue
        if not poly.is_valid:
            poly = poly.buffer(0)
        if not poly.is_empty:
            polys.append(poly)  # type: ignore[arg-type]
    if not polys:
        return None

    geom: BaseGeometry = unary_union(polys)
    scale = font_size / upem
    # Font space: +Y up. SVG: +Y down. Flip about the baseline (y=0).
    geom = affinity.scale(geom, xfact=scale, yfact=-scale, origin=(0.0, 0.0))
    metrics = measure_text(text, font_size)
    if anchor == "middle":
        ox = x - metrics.advance * 0.5
    elif anchor == "end":
        ox = x - metrics.advance
    else:
        ox = x
    return affinity.translate(geom, xoff=ox, yoff=y)


def geom_to_svg_d(geom: BaseGeometry) -> str:
    """Serialize a (Multi)Polygon to an SVG path ``d`` string."""
    if geom.is_empty:
        return ""
    parts: list[BaseGeometry] = []
    if isinstance(geom, Polygon):
        parts = [geom]
    elif isinstance(geom, MultiPolygon):
        parts = list(geom.geoms)
    else:
        parts = [g for g in getattr(geom, "geoms", [geom]) if isinstance(g, Polygon)]

    chunks: list[str] = []
    for poly in parts:
        if poly.is_empty:
            continue

        def _ring(coords: object) -> str:
            pts = list(coords)  # type: ignore[arg-type]
            if len(pts) < 2:
                return ""
            if pts[0] == pts[-1]:
                pts = pts[:-1]
            if not pts:
                return ""
            x0, y0 = pts[0]
            bits = [f"M {x0:.2f} {y0:.2f}"]
            for px, py in pts[1:]:
                bits.append(f"L {px:.2f} {py:.2f}")
            bits.append("Z")
            return " ".join(bits)

        exterior = _ring(poly.exterior.coords)
        if exterior:
            chunks.append(exterior)
        for interior in poly.interiors:
            ring = _ring(interior.coords)
            if ring:
                chunks.append(ring)
    return " ".join(chunks)


def label_halo_path_d(
    text: str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
    buffer_px: float | None = None,
) -> str | None:
    """SVG path for a white knockout grown around the label glyphs.

    Buffer distance defaults to half of ``HALO_STROKE`` so the margin past
    the ink matches a white stroke of width ``HALO_STROKE`` on the outline.
    """
    outline = label_outline(text, x, y, font_size=font_size, anchor=anchor)
    if outline is None or outline.is_empty:
        return None
    dist = HALO_STROKE * 0.5 if buffer_px is None else buffer_px
    grown = outline.buffer(dist, quad_segs=8)
    d = geom_to_svg_d(grown)
    return d or None

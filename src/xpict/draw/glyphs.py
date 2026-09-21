"""Compile Unicode + bold/italic spans to glyph shapes.

One engine for every label: atom ink, molecule captions, edge labels, and
halos. Markup is parsed to :class:`~xpict.draw.richtext.StyledText`
first; this module only sees font-aligned Unicode runs and Liberation Sans
faces (Regular / Bold / Italic / BoldItalic).
"""

from __future__ import annotations

from functools import reduce

from fontTools.pens.transformPen import TransformPen
from shapely import affinity
from shapely.geometry import MultiPolygon, Polygon
from shapely.geometry.base import BaseGeometry
from shapely.ops import unary_union

from xpict.draw.font_face import (
    ContourPen,
    FaceStyle,
    bundled_font_path,  # noqa: F401
    face_style,
    glyph_set_cmap_upem,
)
from xpict.draw.metrics import FONT_PX
from xpict.draw.richtext import StyledText, TextRun, parse_richtext
from xpict.draw.text_metrics import measure_styled


def _contours_to_geom(pen: ContourPen) -> BaseGeometry | None:
    """Build geometry from TrueType contours via XOR (even-odd fill).

    Fonts emit counters (the hole in ``O``, ``A``, …) as separate contours.
    ``symmetric_difference`` punches those holes — no area nesting heuristic.
    """
    raw: list[BaseGeometry] = []
    for contour in pen.contours:
        if len(contour) < 3:
            continue
        poly = Polygon(contour)
        if poly.is_empty:
            continue
        if not poly.is_valid:
            poly = poly.buffer(0)
        if not poly.is_empty:
            raw.append(poly)
    if not raw:
        return None
    geom = reduce(lambda a, b: a.symmetric_difference(b), raw)
    return None if geom.is_empty else geom


def _outline_run_em(
    text: str,
    *,
    style: FaceStyle = "regular",
) -> tuple[BaseGeometry | None, float]:
    """Outline plain Unicode in font space (+Y up); return (geom, advance_em)."""
    if not text:
        return None, 0.0
    glyph_set, cmap, upem = glyph_set_cmap_upem(style)
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
    return _contours_to_geom(pen), pen_x


def compile_text_shapes(
    styled: StyledText | str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
) -> BaseGeometry | None:
    """Compile styled Unicode (or markup string) to SVG-space glyph geometry.

    This is the shared shapes engine: captions, atom labels, edge labels, and
    halos all go through here.
    """
    if isinstance(styled, str):
        styled = StyledText.from_markup(styled)
    if not styled:
        return None

    metrics = measure_styled(styled, font_size)
    if anchor == "middle":
        cursor = x - metrics.advance * 0.5
    elif anchor == "end":
        cursor = x - metrics.advance
    else:
        cursor = x

    parts: list[BaseGeometry] = []
    for run in styled.runs:
        if not run.text:
            continue
        style = face_style(bold=run.bold, italic=run.italic)
        geom_em, advance_em = _outline_run_em(run.text, style=style)
        _gs, _cmap, upem = glyph_set_cmap_upem(style)
        scale = font_size / upem
        run_advance = advance_em * scale
        if geom_em is not None and not geom_em.is_empty:
            geom = affinity.scale(
                geom_em, xfact=scale, yfact=-scale, origin=(0.0, 0.0)
            )
            geom = affinity.translate(geom, xoff=cursor, yoff=y)
            parts.append(geom)
        cursor += run_advance

    if not parts:
        return None
    if len(parts) == 1:
        return parts[0]
    return unary_union(parts)


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


def compile_text_path_d(
    styled: StyledText | str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
) -> str | None:
    """SVG path ``d`` for label ink compiled from styled Unicode / markup."""
    outline = compile_text_shapes(
        styled, x, y, font_size=font_size, anchor=anchor
    )
    if outline is None or outline.is_empty:
        return None
    d = geom_to_svg_d(outline)
    return d or None


def compile_text_halo_d(
    styled: StyledText | str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
    buffer_px: float | None = None,
) -> str | None:
    """White knockout for text — :func:`compile_text_shapes` then ``halo_from_shapes``."""
    from xpict.draw.halo import halo_path_d

    outline = compile_text_shapes(
        styled, x, y, font_size=font_size, anchor=anchor
    )
    return halo_path_d(outline, buffer_px)


def label_outline(
    text: str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
    bold: bool = False,
    italic: bool = False,
) -> BaseGeometry | None:
    """Compile markup/plain text to glyph geometry (shared engine)."""
    if bold or italic:
        styled = StyledText(
            tuple(
                TextRun(run.text, bold=run.bold or bold, italic=run.italic or italic)
                for run in parse_richtext(text)
            )
        )
        return compile_text_shapes(
            styled, x, y, font_size=font_size, anchor=anchor
        )
    return compile_text_shapes(text, x, y, font_size=font_size, anchor=anchor)


def label_path_d(
    text: str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
) -> str | None:
    return compile_text_path_d(text, x, y, font_size=font_size, anchor=anchor)


def label_halo_path_d(
    text: str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
    buffer_px: float | None = None,
) -> str | None:
    return compile_text_halo_d(
        text,
        x,
        y,
        font_size=font_size,
        anchor=anchor,
        buffer_px=buffer_px,
    )


def label_plain(text: str) -> str:
    """Expanded Unicode for ``data-text`` attributes."""
    return StyledText.from_markup(text).text


__all__ = [
    "compile_text_shapes",
    "compile_text_path_d",
    "compile_text_halo_d",
    "geom_to_svg_d",
    "label_outline",
    "label_path_d",
    "label_halo_path_d",
    "label_plain",
]

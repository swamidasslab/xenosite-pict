"""Label glyph outlines from the bundled Liberation Sans face.

Atom labels stay as SVG ``<text>`` for the ink. The white knockout under
them is a **shapely buffer** of the same glyph outlines, serialized as a
path so the halo does not depend on the viewer having the font.
"""

from __future__ import annotations

from functools import lru_cache
from importlib import resources
from pathlib import Path

from fontTools.pens.basePen import BasePen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from shapely.geometry import MultiPolygon, Polygon
from shapely.geometry.base import BaseGeometry
from shapely.ops import unary_union

from xenosite.pict.draw.metrics import FONT_PX, HALO_STROKE

_FONT_PACKAGE = "xenosite.pict.data.fonts"
_FONT_NAME = "LiberationSans-Regular.ttf"
_BEZIER_STEPS = 8


def bundled_font_path() -> Path:
    """Filesystem path to the packaged Liberation Sans Regular TTF."""
    root = resources.files(_FONT_PACKAGE)
    return Path(str(root.joinpath(_FONT_NAME)))


@lru_cache(maxsize=1)
def _font() -> tuple[TTFont, object, dict[int, str], float]:
    path = bundled_font_path()
    font = TTFont(path)
    glyph_set = font.getGlyphSet()
    cmap = font.getBestCmap() or {}
    upem = float(font["head"].unitsPerEm)
    return font, glyph_set, cmap, upem


class _ContourPen(BasePen):
    """Record TrueType contours as polylines (quadratic/cubic sampled)."""

    def __init__(self, glyph_set: object) -> None:
        super().__init__(glyph_set)
        self.contours: list[list[tuple[float, float]]] = []
        self._pts: list[tuple[float, float]] = []

    def _moveTo(self, pt: tuple[float, float]) -> None:
        self._pts = [pt]

    def _lineTo(self, pt: tuple[float, float]) -> None:
        self._pts.append(pt)

    def _curveToOne(
        self,
        p1: tuple[float, float],
        p2: tuple[float, float],
        p3: tuple[float, float],
    ) -> None:
        p0 = self._pts[-1]
        for i in range(1, _BEZIER_STEPS + 1):
            t = i / _BEZIER_STEPS
            mt = 1.0 - t
            x = (
                mt**3 * p0[0]
                + 3 * mt**2 * t * p1[0]
                + 3 * mt * t**2 * p2[0]
                + t**3 * p3[0]
            )
            y = (
                mt**3 * p0[1]
                + 3 * mt**2 * t * p1[1]
                + 3 * mt * t**2 * p2[1]
                + t**3 * p3[1]
            )
            self._pts.append((x, y))

    def _qCurveToOne(self, p1: tuple[float, float], p2: tuple[float, float]) -> None:
        p0 = self._pts[-1]
        for i in range(1, _BEZIER_STEPS + 1):
            t = i / _BEZIER_STEPS
            mt = 1.0 - t
            x = mt**2 * p0[0] + 2 * mt * t * p1[0] + t**2 * p2[0]
            y = mt**2 * p0[1] + 2 * mt * t * p1[1] + t**2 * p2[1]
            self._pts.append((x, y))

    def _closePath(self) -> None:
        if len(self._pts) >= 3:
            self.contours.append(self._pts)
        self._pts = []

    def _endPath(self) -> None:
        self._pts = []


def _advance_width(text: str, upem: float, glyph_set: object, cmap: dict[int, str]) -> float:
    total = 0.0
    for ch in text:
        name = cmap.get(ord(ch))
        if name is None:
            total += 0.5 * upem
            continue
        total += float(glyph_set[name].width)  # type: ignore[index]
    return total


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
    _font_obj, glyph_set, cmap, upem = _font()
    pen = _ContourPen(glyph_set)
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
    from shapely import affinity

    geom = affinity.scale(geom, xfact=scale, yfact=-scale, origin=(0.0, 0.0))
    advance = _advance_width(text, upem, glyph_set, cmap) * scale
    if anchor == "middle":
        ox = x - advance * 0.5
    elif anchor == "end":
        ox = x - advance
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
        # GeometryCollection / others — take polygonal bits.
        parts = [g for g in getattr(geom, "geoms", [geom]) if isinstance(g, Polygon)]

    chunks: list[str] = []
    for poly in parts:
        if poly.is_empty:
            continue
        def _ring(coords: object) -> str:
            pts = list(coords)  # type: ignore[arg-type]
            if len(pts) < 2:
                return ""
            # Drop duplicate close point if present; we emit Z.
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

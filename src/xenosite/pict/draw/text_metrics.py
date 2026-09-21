"""Intrinsic glyph and text-box sizes from the bundled Liberation Sans face.

All em values use font space (+Y up). Pixel helpers use SVG space (+Y down)
with the baseline as the y origin for relative boxes.

These metrics drive bond end-gaps, label placement, and halo buffers so layout
does not rely on a fixed ``0.62 em`` advance guess.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache

from fontTools.pens.boundsPen import BoundsPen
from shapely.geometry import LineString, Polygon
from shapely.ops import unary_union

from xenosite.pict.draw.font_face import ContourPen, glyph_set_cmap_upem, ttfont
from xenosite.pict.draw.metrics import FONT_PX, HALO_STROKE


@dataclass(frozen=True)
class FontFaceMetrics:
    """Typographic constants for the bundled face, in em units."""

    upem: float
    ascent: float  # +Y up (hhea)
    descent: float  # negative in font space
    line_gap: float
    cap_height: float
    x_height: float
    stem_em: float  # vertical stem of H, measured from outline


@dataclass(frozen=True)
class GlyphMetrics:
    """One character of the bundled face, in em units (font +Y up)."""

    char: str
    name: str
    advance: float
    ink_xmin: float | None
    ink_ymin: float | None
    ink_xmax: float | None
    ink_ymax: float | None

    @property
    def has_ink(self) -> bool:
        return self.ink_xmin is not None

    @property
    def ink_width(self) -> float:
        if not self.has_ink:
            return 0.0
        return float(self.ink_xmax) - float(self.ink_xmin)  # type: ignore[arg-type]

    @property
    def ink_height(self) -> float:
        if not self.has_ink:
            return 0.0
        return float(self.ink_ymax) - float(self.ink_ymin)  # type: ignore[arg-type]


@dataclass(frozen=True)
class Box:
    """Axis-aligned box in SVG pixels (+Y down)."""

    xmin: float
    ymin: float
    xmax: float
    ymax: float

    @property
    def width(self) -> float:
        return self.xmax - self.xmin

    @property
    def height(self) -> float:
        return self.ymax - self.ymin

    @property
    def cx(self) -> float:
        return 0.5 * (self.xmin + self.xmax)

    @property
    def cy(self) -> float:
        return 0.5 * (self.ymin + self.ymax)

    def padded(self, pad: float) -> Box:
        return Box(self.xmin - pad, self.ymin - pad, self.xmax + pad, self.ymax + pad)

    def as_tuple(self) -> tuple[float, float, float, float]:
        return (self.xmin, self.ymin, self.xmax, self.ymax)


@dataclass(frozen=True)
class TextMetrics:
    """Laid-out metrics for a string at a pixel font size.

    Relative boxes use origin at the **left of the advance** on the baseline
    (SVG +Y down). Call :meth:`at` to place them with an SVG text anchor.
    """

    text: str
    font_size: float
    advance: float
    ink: Box | None
    typo: Box

    def at(
        self,
        x: float,
        y: float,
        *,
        anchor: str = "middle",
    ) -> tuple[Box | None, Box, float]:
        """Return ``(ink, typo, origin_x)`` with baseline at ``y``.

        ``origin_x`` is the left edge of the advance. ``x`` follows SVG
        ``text-anchor``.
        """
        if anchor == "middle":
            origin_x = x - 0.5 * self.advance
        elif anchor == "end":
            origin_x = x - self.advance
        else:
            origin_x = x
        typo = Box(
            origin_x + self.typo.xmin,
            y + self.typo.ymin,
            origin_x + self.typo.xmax,
            y + self.typo.ymax,
        )
        ink = None
        if self.ink is not None:
            ink = Box(
                origin_x + self.ink.xmin,
                y + self.ink.ymin,
                origin_x + self.ink.xmax,
                y + self.ink.ymax,
            )
        return ink, typo, origin_x

    def clearance(self, *, pad: float | None = None) -> float:
        """Isotropic bond inset for a middle-anchored label on the atom.

        Half of the larger of advance and ink width, plus pad (default half of
        ``HALO_STROKE`` so the stroke stops outside the white knockout).
        """
        margin = HALO_STROKE * 0.5 if pad is None else pad
        half = 0.5 * self.advance
        if self.ink is not None:
            half = max(half, 0.5 * self.ink.width)
        return half + margin


@lru_cache(maxsize=1)
def face_metrics() -> FontFaceMetrics:
    font = ttfont()
    upem = float(font["head"].unitsPerEm)
    hhea = font["hhea"]
    os2 = font["OS/2"]
    cap = float(getattr(os2, "sCapHeight", 0) or 0)
    xh = float(getattr(os2, "sxHeight", 0) or 0)
    if cap <= 0:
        g = glyph_metrics("H")
        cap = float(g.ink_ymax or 0.0)
    if xh <= 0:
        g = glyph_metrics("x")
        xh = float(g.ink_ymax or 0.0)
    return FontFaceMetrics(
        upem=upem,
        ascent=float(hhea.ascent),
        descent=float(hhea.descent),
        line_gap=float(hhea.lineGap),
        cap_height=cap,
        x_height=xh,
        stem_em=_measure_stem_em(),
    )


@lru_cache(maxsize=512)
def glyph_metrics(char: str) -> GlyphMetrics:
    """Metrics for a single Unicode character (first codepoint if longer)."""
    if not char:
        return GlyphMetrics("", ".null", 0.0, None, None, None, None)
    ch = char[0]
    glyph_set, cmap, upem = glyph_set_cmap_upem()
    name = cmap.get(ord(ch))
    if name is None:
        return GlyphMetrics(ch, ".notdef", 0.5 * upem, None, None, None, None)
    glyph = glyph_set[name]  # type: ignore[index]
    bp = BoundsPen(glyph_set)
    glyph.draw(bp)
    if bp.bounds is None:
        return GlyphMetrics(ch, name, float(glyph.width), None, None, None, None)
    xmin, ymin, xmax, ymax = bp.bounds
    return GlyphMetrics(
        ch,
        name,
        float(glyph.width),
        float(xmin),
        float(ymin),
        float(xmax),
        float(ymax),
    )


def _measure_stem_em() -> float:
    """Vertical stem width of H in em, via outline coverage at mid-cap."""
    glyph_set, cmap, upem = glyph_set_cmap_upem()
    name = cmap.get(ord("H"))
    if name is None:
        return 0.0933
    pen = ContourPen(glyph_set)
    glyph_set[name].draw(pen)  # type: ignore[index]
    polys = []
    for contour in pen.contours:
        if len(contour) < 3:
            continue
        p = Polygon(contour)
        if not p.is_valid:
            p = p.buffer(0)
        if not p.is_empty:
            polys.append(p)
    if not polys:
        return 0.0933
    geom = unary_union(polys)
    minx, miny, maxx, maxy = geom.bounds
    y = miny + 0.75 * (maxy - miny)
    chord = LineString([(minx - 1.0, y), (maxx + 1.0, y)])
    hit = geom.intersection(chord)
    if hit.is_empty:
        return 0.0933
    if hit.geom_type == "MultiLineString":
        seg = min(hit.geoms, key=lambda g: g.bounds[0])
        return float(seg.length) / upem
    if hit.geom_type == "LineString":
        return float(hit.length) / upem
    return 0.0933


@lru_cache(maxsize=256)
def measure_text(text: str, font_size: float = FONT_PX) -> TextMetrics:
    """Advance, ink box, and typographic box for ``text`` at ``font_size``."""
    face = face_metrics()
    scale = font_size / face.upem
    advance_em = 0.0
    ink_xmin = ink_ymin = ink_xmax = ink_ymax = None
    x_cursor = 0.0
    for ch in text:
        g = glyph_metrics(ch)
        if g.has_ink:
            gx0 = x_cursor + float(g.ink_xmin)
            gy0 = float(g.ink_ymin)
            gx1 = x_cursor + float(g.ink_xmax)
            gy1 = float(g.ink_ymax)
            ink_xmin = gx0 if ink_xmin is None else min(ink_xmin, gx0)
            ink_ymin = gy0 if ink_ymin is None else min(ink_ymin, gy0)
            ink_xmax = gx1 if ink_xmax is None else max(ink_xmax, gx1)
            ink_ymax = gy1 if ink_ymax is None else max(ink_ymax, gy1)
        x_cursor += g.advance
        advance_em += g.advance

    typo_em = (0.0, face.descent, advance_em or 0.5 * face.upem, face.ascent)

    def _to_svg_box(xmin: float, ymin: float, xmax: float, ymax: float) -> Box:
        return Box(xmin * scale, -ymax * scale, xmax * scale, -ymin * scale)

    ink_box = None
    if ink_xmin is not None:
        ink_box = _to_svg_box(ink_xmin, ink_ymin, ink_xmax, ink_ymax)  # type: ignore[arg-type]
    typo_box = _to_svg_box(*typo_em)
    return TextMetrics(
        text=text,
        font_size=font_size,
        advance=advance_em * scale,
        ink=ink_box,
        typo=typo_box,
    )


def text_box(
    text: str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
    pad: float = 0.0,
    which: str = "ink",
) -> Box:
    """Absolute SVG box for ``text`` with baseline at ``(x, y)`` per anchor.

    ``which`` is ``\"ink\"`` (tight glyph bounds) or ``\"typo\"`` (ascender line).
    Falls back to typo when the run has no ink.
    """
    m = measure_text(text, font_size)
    ink, typo, _ = m.at(x, y, anchor=anchor)
    box = ink if which == "ink" and ink is not None else typo
    return box.padded(pad) if pad else box


def label_baseline_offset(font_size: float = FONT_PX) -> float:
    """Baseline shift so cap-ink centers on the atom (SVG +Y down)."""
    face = face_metrics()
    return 0.5 * (face.cap_height / face.upem) * font_size


def stem_width(font_size: float = FONT_PX) -> float:
    """Vertical stem width of H at ``font_size`` (matches default bond stroke)."""
    return face_metrics().stem_em * font_size

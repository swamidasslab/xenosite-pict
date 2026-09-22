"""Intrinsic glyph and text-box sizes from the bundled Liberation Sans face.

All em values use font space (+Y up). Pixel helpers use SVG space (+Y down)
with the baseline as the y origin for relative boxes.

Metrics come from the Rust ``ttf-parser`` stack (``xpict._native``).
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache

from xpict import _native
from xpict.draw.font_face import FaceStyle, face_style
from xpict.draw.metrics import FONT_PX, LABEL_GAP_PX
from xpict.draw.richtext import StyledText, TextRun


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
        """Return ``(ink, typo, origin_x)`` with baseline at ``y``."""
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
        """Isotropic bond inset for a middle-anchored label on the atom."""
        margin = LABEL_GAP_PX if pad is None else pad
        half = 0.5 * self.advance
        if self.ink is not None:
            half = max(half, 0.5 * self.ink.width)
        return half + margin


@lru_cache(maxsize=4)
def face_metrics(style: FaceStyle = "regular") -> FontFaceMetrics:
    upem, ascent, descent, line_gap, cap, xh, stem = _native.face_metrics(style)
    return FontFaceMetrics(
        upem=float(upem),
        ascent=float(ascent),
        descent=float(descent),
        line_gap=float(line_gap),
        cap_height=float(cap),
        x_height=float(xh),
        stem_em=float(stem),
    )


@lru_cache(maxsize=2048)
def glyph_metrics(char: str, style: FaceStyle = "regular") -> GlyphMetrics:
    """Metrics for a single Unicode character (first codepoint if longer)."""
    if not char:
        return GlyphMetrics("", ".null", 0.0, None, None, None, None)
    ch = char[0]
    advance, ink = _native.glyph_metrics(ch, style)
    if ink is None:
        return GlyphMetrics(ch, ch, float(advance), None, None, None, None)
    xmin, ymin, xmax, ymax = ink
    return GlyphMetrics(
        ch,
        ch,
        float(advance),
        float(xmin),
        float(ymin),
        float(xmax),
        float(ymax),
    )


def measure_styled(styled: StyledText, font_size: float = FONT_PX) -> TextMetrics:
    """Metrics for :class:`StyledText` (Unicode + bold/italic spans)."""
    return measure_runs(styled.runs, font_size)


@lru_cache(maxsize=256)
def measure_text(text: str, font_size: float = FONT_PX) -> TextMetrics:
    """Advance / boxes for markup or plain text at ``font_size``."""
    return measure_styled(StyledText.from_markup(text), font_size)


def _measure_plain(
    text: str, font_size: float, *, style: FaceStyle = "regular"
) -> TextMetrics:
    face = face_metrics(style)
    scale = font_size / face.upem
    advance_em = 0.0
    ink_xmin = ink_ymin = ink_xmax = ink_ymax = None
    x_cursor = 0.0
    for ch in text:
        g = glyph_metrics(ch, style)
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


@lru_cache(maxsize=256)
def measure_runs(
    runs: tuple[TextRun, ...], font_size: float = FONT_PX
) -> TextMetrics:
    """Metrics for styled runs laid out on one baseline."""
    if not runs:
        return _measure_plain("", font_size)
    if len(runs) == 1 and not runs[0].bold and not runs[0].italic:
        return _measure_plain(runs[0].text, font_size, style="regular")

    face = face_metrics()
    scale = font_size / face.upem
    advance_em = 0.0
    ink_xmin = ink_ymin = ink_xmax = ink_ymax = None
    x_cursor = 0.0
    plain_parts: list[str] = []
    for run in runs:
        style = face_style(bold=run.bold, italic=run.italic)
        plain_parts.append(run.text)
        for ch in run.text:
            g = glyph_metrics(ch, style)
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
    return TextMetrics(
        text="".join(plain_parts),
        font_size=font_size,
        advance=advance_em * scale,
        ink=ink_box,
        typo=_to_svg_box(*typo_em),
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
    """Absolute SVG box for ``text`` with baseline at ``(x, y)`` per anchor."""
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

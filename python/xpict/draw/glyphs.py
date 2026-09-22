"""Compile Unicode + bold/italic spans to glyph shapes.

One engine for every label: atom ink, molecule captions, edge labels, and
halos. Markup is parsed to :class:`~xpict.draw.richtext.StyledText`
first; outlines come from the Rust Liberation Sans stack (``xpict._native``).
"""

from __future__ import annotations

from xpict import _native
from xpict.draw.font_face import FaceStyle, face_style
from xpict.draw.metrics import FONT_PX
from xpict.draw.richtext import StyledText, TextRun, parse_richtext
from xpict.draw.text_metrics import measure_styled
from xpict.native_bridge import Shape


def _outline_run_em(
    text: str,
    *,
    style: FaceStyle = "regular",
) -> tuple[Shape | None, float]:
    """Outline plain Unicode in font space (+Y up); return (geom, advance_em)."""
    if not text:
        return None, 0.0
    shape, advance = _native.outline_run_em(text, style)
    return shape, float(advance)


def compile_text_shapes(
    styled: StyledText | str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
) -> Shape | None:
    """Compile styled Unicode (or markup string) to SVG-space glyph geometry."""
    if isinstance(styled, str):
        styled = StyledText.from_markup(styled)
    if not styled:
        return None

    # Fast path: single unstyled run → one native call.
    if len(styled.runs) == 1 and not styled.runs[0].bold and not styled.runs[0].italic:
        return _native.compile_text_shapes(styled.runs[0].text, x, y, font_size, anchor, "regular")

    metrics = measure_styled(styled, font_size)
    if anchor == "middle":
        cursor = x - metrics.advance * 0.5
    elif anchor == "end":
        cursor = x - metrics.advance
    else:
        cursor = x

    parts: list[Shape] = []
    for run in styled.runs:
        if not run.text:
            continue
        style = face_style(bold=run.bold, italic=run.italic)
        geom_em, advance_em = _outline_run_em(run.text, style=style)
        face = _native.face_metrics(style)
        upem = face[0]
        scale = font_size / upem
        run_advance = advance_em * scale
        if geom_em is not None and not geom_em.is_empty:
            geom = geom_em.scale(scale, -scale, 0.0, 0.0).translate(cursor, y)
            parts.append(geom)
        cursor += run_advance

    if not parts:
        return None
    acc = parts[0]
    for p in parts[1:]:
        acc = acc.union(p)
    return None if acc.is_empty else acc


def geom_to_svg_d(geom: Shape) -> str:
    """Serialize a Shape to an SVG path ``d`` string."""
    if geom.is_empty:
        return ""
    return geom.to_svg_d()


def compile_text_path_d(
    styled: StyledText | str,
    x: float,
    y: float,
    *,
    font_size: float = FONT_PX,
    anchor: str = "middle",
) -> str | None:
    """SVG path ``d`` for label ink compiled from styled Unicode / markup."""
    outline = compile_text_shapes(styled, x, y, font_size=font_size, anchor=anchor)
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
    """White knockout for text — :func:`compile_text_shapes` then ``halo_path_d``."""
    from xpict.draw.halo import halo_path_d

    outline = compile_text_shapes(styled, x, y, font_size=font_size, anchor=anchor)
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
) -> Shape | None:
    """Compile markup/plain text to glyph geometry (shared engine)."""
    if bold or italic:
        styled = StyledText(
            tuple(
                TextRun(run.text, bold=run.bold or bold, italic=run.italic or italic)
                for run in parse_richtext(text)
            )
        )
        return compile_text_shapes(styled, x, y, font_size=font_size, anchor=anchor)
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

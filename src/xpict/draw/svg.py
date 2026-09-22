"""Serialize Scene → SVG / HTML.

Text primitives are drawn as glyph **paths** (Liberation Sans outlines), not
``<text>``/``<tspan>``. Markup (``\\alpha``, ``**bold**``, …) is expanded when
outlining so bold/italic pick the matching bundled face.
"""

from __future__ import annotations

import html
from xml.etree.ElementTree import Element, SubElement, tostring

from xpict.contracts.scene import (
    CirclePrim,
    PathPrim,
    Primitive,
    Scene,
    TextPrim,
    Viewport,
)
from xpict.draw.glyphs import compile_text_path_d
from xpict.draw.richtext import StyledText


def _render_primitive(parent: Element, prim: Primitive) -> None:
    if isinstance(prim, PathPrim):
        attrs = {
            "d": prim.d,
            "fill": prim.fill or "none",
            "stroke": prim.stroke or "none",
            "stroke-width": str(prim.stroke_width),
            "stroke-linecap": prim.stroke_linecap or "round",
            "stroke-linejoin": "round",
            "opacity": str(prim.opacity),
        }
        if prim.stroke_dasharray:
            attrs["stroke-dasharray"] = prim.stroke_dasharray
        if prim.cls:
            attrs["class"] = prim.cls
        SubElement(parent, "path", attrs)
    elif isinstance(prim, CirclePrim):
        attrs = {
            "cx": f"{prim.cx:.2f}",
            "cy": f"{prim.cy:.2f}",
            "r": f"{prim.r:.2f}",
            "opacity": str(prim.opacity),
            "fill": prim.fill or "none",
        }
        if prim.stroke:
            attrs["stroke"] = prim.stroke
            attrs["stroke-width"] = str(prim.stroke_width)
        if prim.cls:
            attrs["class"] = prim.cls
        SubElement(parent, "circle", attrs)
    elif isinstance(prim, TextPrim):
        # Markup → StyledText (Unicode + spans) → glyph path shapes.
        styled = StyledText.from_markup(prim.text)
        d = compile_text_path_d(
            styled,
            prim.x,
            prim.y,
            font_size=prim.font_size,
            anchor=prim.anchor,
        )
        if not d:
            return
        attrs = {
            "d": d,
            "fill": prim.fill,
            "stroke": "none",
            "opacity": "1",
            "data-text": styled.text,
        }
        if prim.cls:
            attrs["class"] = prim.cls
        SubElement(parent, "path", attrs)


def _render_viewport(
    parent: Element,
    vp: Viewport,
    *,
    layers: tuple[str, ...] | None = None,
) -> None:
    g = SubElement(
        parent, "g", {"class": "xpict-mol", "transform": f"translate({vp.x},{vp.y})"}
    )
    if vp.id:
        g.set("data-id", vp.id)
    want = set(layers) if layers is not None else None
    for layer in vp.layers:
        if want is not None and layer.name not in want:
            continue
        if not layer.primitives:
            continue
        lg = SubElement(g, "g", {"class": f"layer-{layer.name}", "id": layer.name})
        for prim in layer.primitives:
            _render_primitive(lg, prim)


def _fmt_user(value: float) -> str:
    """Format a user-unit length (viewBox / geometry), trimming float noise."""
    return f"{value:.4f}".rstrip("0").rstrip(".") or "0"


def _fmt_css_px(value: float) -> str:
    """Format an intrinsic CSS size so browsers lay the SVG out at content size."""
    return f"{_fmt_user(value)}px"


def scene_to_svg(scene: Scene) -> str:
    # Explicit px width/height give a stable intrinsic size. viewBox stays in
    # user units so content scales only when CSS intentionally overrides size.
    w = _fmt_user(scene.width)
    h = _fmt_user(scene.height)
    root = Element(
        "svg",
        {
            "xmlns": "http://www.w3.org/2000/svg",
            "width": _fmt_css_px(scene.width),
            "height": _fmt_css_px(scene.height),
            "viewBox": f"0 0 {w} {h}",
            "class": "xpict",
        },
    )
    # Shade under the document halo; ink above it.
    for vp in scene.viewports:
        _render_viewport(root, vp, layers=("shading",))
    if scene.halo:
        hg = SubElement(root, "g", {"class": "xpict-halo", "id": "halo"})
        for prim in scene.halo:
            _render_primitive(hg, prim)
    for vp in scene.viewports:
        _render_viewport(root, vp, layers=("bonds", "labels", "marks", "overlay"))
    if scene.overlays:
        og = SubElement(root, "g", {"class": "xpict-overlays", "id": "overlays"})
        for prim in scene.overlays:
            _render_primitive(og, prim)
    return '<?xml version="1.0" encoding="UTF-8"?>\n' + tostring(root, encoding="unicode")


def scene_to_html(scene: Scene, *, title: str | None = None) -> str:
    svg = scene_to_svg(scene)
    if svg.startswith("<?xml"):
        svg = svg.split("\n", 1)[1]
    t = html.escape(title or "xpict")
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{t}</title>
<style>
  body {{ margin: 0; font-family: system-ui, sans-serif; background: #fafafa; }}
  .xpict-page {{ padding: 1rem; box-sizing: border-box; }}
  /* Honor SVG width/height attrs (intrinsic size). Shrink only if the page is
     narrower — never stretch separate depictions up to a shared column width. */
  .xpict-page svg.xpict {{
    display: inline-block;
    vertical-align: middle;
    width: auto;
    height: auto;
    max-width: 100%;
    flex: 0 0 auto;
  }}
  .xpict-row {{
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1rem;
  }}
</style>
</head>
<body>
<main class="xpict-page">
{svg}
</main>
</body>
</html>
"""

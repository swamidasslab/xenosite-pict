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


def _render_viewport(parent: Element, vp: Viewport) -> None:
    g = SubElement(
        parent, "g", {"class": "xpict-mol", "transform": f"translate({vp.x},{vp.y})"}
    )
    if vp.id:
        g.set("data-id", vp.id)
    for layer in vp.layers:
        lg = SubElement(g, "g", {"class": f"layer-{layer.name}", "id": layer.name})
        for prim in layer.primitives:
            _render_primitive(lg, prim)


def scene_to_svg(scene: Scene) -> str:
    root = Element(
        "svg",
        {
            "xmlns": "http://www.w3.org/2000/svg",
            "width": str(scene.width),
            "height": str(scene.height),
            "viewBox": f"0 0 {scene.width} {scene.height}",
            "class": "xpict",
        },
    )
    for vp in scene.viewports:
        _render_viewport(root, vp)
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
  .xpict-page {{ max-width: 100%; padding: 1rem; box-sizing: border-box; }}
  .xpict-page svg {{ max-width: 100%; height: auto; display: block; }}
</style>
</head>
<body>
<main class="xpict-page">
{svg}
</main>
</body>
</html>
"""

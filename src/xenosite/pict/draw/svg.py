"""Serialize Scene → SVG / HTML."""

from __future__ import annotations

import html
from xml.etree.ElementTree import Element, SubElement, tostring

from xenosite.pict.contracts.scene import CirclePrim, PathPrim, Scene, TextPrim, Viewport


def _render_viewport(parent: Element, vp: Viewport) -> None:
    g = SubElement(
        parent, "g", {"class": "pict-mol", "transform": f"translate({vp.x},{vp.y})"}
    )
    if vp.id:
        g.set("data-id", vp.id)
    for layer in vp.layers:
        lg = SubElement(g, "g", {"class": f"layer-{layer.name}", "id": layer.name})
        for prim in layer.primitives:
            if isinstance(prim, PathPrim):
                attrs = {
                    "d": prim.d,
                    "fill": prim.fill or "none",
                    "stroke": prim.stroke or "none",
                    "stroke-width": str(prim.stroke_width),
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "opacity": str(prim.opacity),
                }
                if prim.cls:
                    attrs["class"] = prim.cls
                SubElement(lg, "path", attrs)
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
                SubElement(lg, "circle", attrs)
            elif isinstance(prim, TextPrim):
                attrs = {
                    "x": f"{prim.x:.2f}",
                    "y": f"{prim.y:.2f}",
                    "fill": prim.fill,
                    "font-size": str(prim.font_size),
                    "font-family": prim.font_family,
                    "text-anchor": prim.anchor,
                }
                if prim.cls:
                    attrs["class"] = prim.cls
                t = SubElement(lg, "text", attrs)
                t.text = prim.text


def scene_to_svg(scene: Scene) -> str:
    root = Element(
        "svg",
        {
            "xmlns": "http://www.w3.org/2000/svg",
            "width": str(scene.width),
            "height": str(scene.height),
            "viewBox": f"0 0 {scene.width} {scene.height}",
            "class": "xenosite-pict",
        },
    )
    for vp in scene.viewports:
        _render_viewport(root, vp)
    return '<?xml version="1.0" encoding="UTF-8"?>\n' + tostring(root, encoding="unicode")


def scene_to_html(scene: Scene, *, title: str | None = None) -> str:
    svg = scene_to_svg(scene)
    if svg.startswith("<?xml"):
        svg = svg.split("\n", 1)[1]
    t = html.escape(title or "xenosite.pict")
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{t}</title>
<style>
  body {{ margin: 0; font-family: system-ui, sans-serif; background: #fafafa; }}
  .pict-page {{ max-width: 100%; padding: 1rem; box-sizing: border-box; }}
  .pict-page svg {{ max-width: 100%; height: auto; display: block; background: #fff; }}
</style>
</head>
<body>
<main class="pict-page">
{svg}
</main>
</body>
</html>
"""

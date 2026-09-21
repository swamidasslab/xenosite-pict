"""Serialize Scene → SVG / HTML."""

from __future__ import annotations

import html
from xml.etree.ElementTree import Element, SubElement, tostring

from xenosite.pict.contracts.scene import (
    CirclePrim,
    PathPrim,
    Primitive,
    Scene,
    TextPrim,
    Viewport,
)
from xenosite.pict.draw.richtext import TextRun, parse_richtext


def _append_text_runs(parent: Element, runs: list[TextRun]) -> None:
    """Emit plain text and styled ``tspan`` children for rich runs."""
    if not runs:
        return
    # Single unstyled run → keep simple text content (no tspan noise).
    if len(runs) == 1 and not runs[0].bold and not runs[0].italic:
        parent.text = runs[0].text
        return
    # Mixed styles: put leading plain in .text, rest as tspans (SVG convention).
    first = True
    for run in runs:
        if first and not run.bold and not run.italic:
            parent.text = run.text
            first = False
            continue
        attrs: dict[str, str] = {}
        if run.bold:
            attrs["font-weight"] = "bold"
        if run.italic:
            attrs["font-style"] = "italic"
        span = SubElement(parent, "tspan", attrs)
        span.text = run.text
        first = False


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
        t = SubElement(parent, "text", attrs)
        _append_text_runs(t, parse_richtext(prim.text))


def _render_viewport(parent: Element, vp: Viewport) -> None:
    g = SubElement(
        parent, "g", {"class": "pict-mol", "transform": f"translate({vp.x},{vp.y})"}
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
            "class": "xenosite-pict",
        },
    )
    for vp in scene.viewports:
        _render_viewport(root, vp)
    if scene.overlays:
        og = SubElement(root, "g", {"class": "pict-overlays", "id": "overlays"})
        for prim in scene.overlays:
            _render_primitive(og, prim)
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
  .pict-page svg {{ max-width: 100%; height: auto; display: block; }}
</style>
</head>
<body>
<main class="pict-page">
{svg}
</main>
</body>
</html>
"""

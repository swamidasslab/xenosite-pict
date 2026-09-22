"""Serialize Scene → SVG / HTML.

Text primitives are drawn as glyph **paths** (Liberation Sans outlines), not
``<text>``/``<tspan>``. Markup (``\\alpha``, ``**bold**``, …) is expanded when
outlining so bold/italic pick the matching bundled face.

HTML embeds the SVG as a ``data:image/svg+xml`` ``<img>`` so the browser
treats width/height as a replaced-element intrinsic size (no inline-SVG CSS
stretch when several depictions sit side by side).
"""

from __future__ import annotations

import html
import re
from urllib.parse import quote
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

_SVG_ROOT_ATTR = re.compile(r"<svg\b([^>]*)>", re.IGNORECASE | re.DOTALL)
_SVG_ATTR = re.compile(r'([\w:-]+)="([^"]*)"')
# Path `d` numbers (and any float noise on attrs) → compact 2dp on emit.
_SVG_NUM = re.compile(r"[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?")


def _fmt_num(value: float) -> str:
    """Drawing units → SVG text: 2 decimal places, trim trailing zeros."""
    if value != value:  # NaN
        return "0"
    s = f"{value:.2f}"
    if "." in s:
        s = s.rstrip("0").rstrip(".")
    return s or "0"


def _fmt_path_d(d: str) -> str:
    """Round every number in a path `d` to 2dp (Scene may keep full floats)."""
    return _SVG_NUM.sub(lambda m: _fmt_num(float(m.group(0))), d)


def _render_primitive(parent: Element, prim: Primitive) -> None:
    if isinstance(prim, PathPrim):
        attrs = {
            "d": _fmt_path_d(prim.d),
            "fill": prim.fill or "none",
            "stroke": prim.stroke or "none",
            "stroke-width": _fmt_num(prim.stroke_width),
            "stroke-linecap": prim.stroke_linecap or "round",
            "stroke-linejoin": "round",
            "opacity": _fmt_num(prim.opacity),
        }
        if prim.stroke_dasharray:
            attrs["stroke-dasharray"] = prim.stroke_dasharray
        if prim.cls:
            attrs["class"] = prim.cls
        SubElement(parent, "path", attrs)
    elif isinstance(prim, CirclePrim):
        attrs = {
            "cx": _fmt_num(prim.cx),
            "cy": _fmt_num(prim.cy),
            "r": _fmt_num(prim.r),
            "opacity": _fmt_num(prim.opacity),
            "fill": prim.fill or "none",
        }
        if prim.stroke:
            attrs["stroke"] = prim.stroke
            attrs["stroke-width"] = _fmt_num(prim.stroke_width)
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
            "d": _fmt_path_d(d),
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
    want = set(layers) if layers is not None else None
    prims: list[tuple[str, list]] = []
    for layer in vp.layers:
        if want is not None and layer.name not in want:
            continue
        if not layer.primitives:
            continue
        prims.append((layer.name, layer.primitives))
    if not prims:
        return
    g = SubElement(
        parent,
        "g",
        {
            "class": "xpict-mol",
            "transform": f"translate({_fmt_num(vp.x)},{_fmt_num(vp.y)})",
        },
    )
    if vp.id:
        g.set("data-id", vp.id)
    for name, primitives in prims:
        lg = SubElement(g, "g", {"class": f"layer-{name}", "id": name})
        for prim in primitives:
            _render_primitive(lg, prim)


def _fmt_user(value: float) -> str:
    """Format a user-unit length (viewBox / width / height) to 2dp."""
    return _fmt_num(value)


def _strip_xml_decl(svg: str) -> str:
    if svg.startswith("<?xml"):
        return svg.split("\n", 1)[1] if "\n" in svg else svg.split("?>", 1)[-1]
    return svg


def svg_root_size_px(svg: str) -> tuple[str, str] | None:
    """Return ``(width, height)`` numeric strings from an SVG root, if present."""
    m = _SVG_ROOT_ATTR.search(svg)
    if not m:
        return None
    attrs = dict(_SVG_ATTR.findall(m.group(1)))
    w = attrs.get("width", "").removesuffix("px")
    h = attrs.get("height", "").removesuffix("px")
    if not w or not h:
        return None
    return w, h


def svg_to_data_uri(svg: str) -> str:
    """Encode an SVG document as a ``data:image/svg+xml`` URI for ``<img src>``.

    Matches xenosite.org: ``data:image/svg+xml;utf8,…`` with percent-encoding.
    The SVG’s own ``width``/``height`` become the image’s intrinsic size.
    """
    body = _strip_xml_decl(svg).strip()
    return "data:image/svg+xml;utf8," + quote(body, safe="")


def svg_to_img_tag(
    svg: str,
    *,
    alt: str = "xpict",
    cls: str = "xpict",
) -> str:
    """Wrap ``svg`` in an ``<img>`` whose ``src`` is a data URI.

    ``width``/``height`` attributes mirror the SVG root so layout is correct
    before (and without relying on) CSS — same pattern as xenosite’s
    ``interactive-molecule__img``.
    """
    uri = svg_to_data_uri(svg)
    size = svg_root_size_px(svg)
    size_attrs = f' width="{size[0]}" height="{size[1]}"' if size else ""
    return (
        f'<img class="{html.escape(cls)}" src="{uri}"{size_attrs} '
        f'alt="{html.escape(alt)}" decoding="async" draggable="false"/>'
    )


def scene_to_svg(scene: Scene) -> str:
    # width/height match viewBox (drawing units = SCALE-space). As a data-URI
    # <img>, browsers treat those numbers as intrinsic CSS px — xenosite.org
    # acetone uses the same pattern (width="103.0" height="97.2").
    w = _fmt_user(scene.width)
    h = _fmt_user(scene.height)
    root = Element(
        "svg",
        {
            "xmlns": "http://www.w3.org/2000/svg",
            "width": w,
            "height": h,
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
    t = title or "xpict"
    img = svg_to_img_tag(scene_to_svg(scene), alt=t)
    te = html.escape(t)
    # CSS mirrors xenosite.org ``.interactive-molecule`` /
    # ``.interactive-molecule__img``: intrinsic size from SVG width/height,
    # shrink-only via max-width (never stretch separate mols to one column).
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{te}</title>
<style>
  body {{ margin: 0; font-family: system-ui, sans-serif; background: #fafafa; color: #111; }}
  .xpict-page {{ padding: 1rem; box-sizing: border-box; }}
  .xpict-mol {{
    position: relative;
    display: inline-block;
    max-width: 100%;
    line-height: 0;
  }}
  .xpict-mol img.xpict {{
    display: block;
    max-width: 100%;
    height: auto;
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
<div class="xpict-mol">
{img}
</div>
</main>
</body>
</html>
"""

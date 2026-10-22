# Auto-generated from Rust schemars (make types) — do not edit.
"""Drawable scene graph — live ABI from xpict-core (schemars). TypedDict wire forms (no ``dict[str, Any]``)."""

from __future__ import annotations

from typing import Literal, NotRequired, Required, TypedDict

class PathPrimDict(TypedDict, total=False):
    kind: Required[Literal['path']]
    d: Required[str]
    cls: NotRequired[str]
    data_text: NotRequired[str]
    fill: NotRequired[str]
    opacity: NotRequired[float]
    stroke: NotRequired[str]
    stroke_dasharray: NotRequired[str]
    stroke_linecap: NotRequired[str]
    stroke_width: NotRequired[float]


class CirclePrimDict(TypedDict, total=False):
    kind: Required[Literal['circle']]
    cx: Required[float]
    cy: Required[float]
    r: Required[float]
    cls: NotRequired[str]
    fill: NotRequired[str]
    opacity: NotRequired[float]
    stroke: NotRequired[str]
    stroke_width: NotRequired[float]


TextAnchor = Literal['start', 'middle', 'end']


class TextPrimDict(TypedDict, total=False):
    """Logical text; serializers outline to glyph paths (Liberation) or emit `<text>` when the host prefers."""
    kind: Required[Literal['text']]
    text: Required[str]
    x: Required[float]
    y: Required[float]
    anchor: NotRequired[TextAnchor]
    cls: NotRequired[str]
    fill: NotRequired[str]
    font_size: NotRequired[float]
    rotate: NotRequired[float]


PrimitiveDict = PathPrimDict | CirclePrimDict | TextPrimDict


LayerName = Literal['shading', 'halo', 'bonds', 'labels', 'marks', 'overlay']


class LayerDict(TypedDict, total=False):
    name: Required[LayerName]
    primitives: NotRequired[list[PrimitiveDict]]


class ViewportDict(TypedDict, total=False):
    """One molecule viewport in a diagram (after ELK/grid placement)."""
    height: Required[float]
    width: Required[float]
    id: NotRequired[str]
    layers: NotRequired[list[LayerDict]]
    ox: NotRequired[float]
    oy: NotRequired[float]
    x: NotRequired[float]
    y: NotRequired[float]


class SceneDict(TypedDict, total=False):
    """Full drawable document before SVG/HTML serialization."""
    height: Required[float]
    viewports: Required[list[ViewportDict]]
    width: Required[float]
    halo: NotRequired[list[PrimitiveDict]]
    overlays: NotRequired[list[PrimitiveDict]]
    x: NotRequired[float]
    y: NotRequired[float]

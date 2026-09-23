# Auto-generated from Rust schemars (make types) — do not edit.
"""Drawable scene graph — live ABI from xpict-core (schemars)."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, RootModel


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    def model_dump(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump(*args, **kwargs)

    def model_dump_json(self, *args, **kwargs):
        kwargs.setdefault("exclude_none", True)
        return super().model_dump_json(*args, **kwargs)


TextAnchor = Literal['start', 'middle', 'end']


LayerName = Literal['shading', 'halo', 'bonds', 'labels', 'marks', 'overlay']


class PathPrim(StrictModel):
    kind: Literal['path'] = 'path'
    d: str
    cls: str | None = None
    data_text: str | None = Field(None, description='Plain label string for ``data-text`` (glyph paths only).')
    fill: str | None = None
    opacity: float = 1.0
    stroke: str | None = None
    stroke_dasharray: str | None = None
    stroke_linecap: str | None = None
    stroke_width: float = 1.5


class CirclePrim(StrictModel):
    kind: Literal['circle'] = 'circle'
    cx: float
    cy: float
    r: float
    cls: str | None = None
    fill: str | None = None
    opacity: float = 1.0
    stroke: str | None = None
    stroke_width: float = 1.5


class TextPrim(StrictModel):
    """Logical text; serializers outline to glyph paths (Liberation) or emit `<text>` when the host prefers."""
    kind: Literal['text'] = 'text'
    text: str
    x: float
    y: float
    anchor: TextAnchor = 'middle'
    cls: str | None = None
    fill: str = '#000'
    font_size: float = 12.0


Primitive = Annotated[PathPrim | CirclePrim | TextPrim, Field(discriminator='kind')]


class Layer(StrictModel):
    name: LayerName
    primitives: list[Primitive] = Field(default_factory=list)


class Viewport(StrictModel):
    """One molecule viewport in a diagram (after ELK/grid placement)."""
    height: float
    width: float
    id: str | None = None
    layers: list[Layer] = Field(default_factory=list)
    x: float = 0.0
    y: float = 0.0


class Scene(StrictModel):
    """Full drawable document before SVG/HTML serialization."""
    height: float
    viewports: list[Viewport]
    width: float
    halo: list[Primitive] = Field(default_factory=list, description='Single document-space unioned knockout under molecule ink.')
    overlays: list[Primitive] = Field(default_factory=list, description='Document-space primitives (reaction arrows) above viewports.')

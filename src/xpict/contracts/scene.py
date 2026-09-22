"""Drawable scene graph contract (engine-neutral)."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


LayerName = Literal["shading", "halo", "bonds", "labels", "marks", "overlay"]


class Point(StrictModel):
    x: float
    y: float


class PathPrim(StrictModel):
    kind: Literal["path"] = "path"
    d: str
    stroke: str | None = "#000"
    fill: str | None = "none"
    stroke_width: float = 1.5
    opacity: float = 1.0
    stroke_dasharray: str | None = None
    stroke_linecap: Literal["butt", "round", "square"] | None = None
    cls: str | None = None


class CirclePrim(StrictModel):
    kind: Literal["circle"] = "circle"
    cx: float
    cy: float
    r: float
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float = 1.0
    opacity: float = 1.0
    cls: str | None = None


class TextPrim(StrictModel):
    kind: Literal["text"] = "text"
    x: float
    y: float
    text: str
    fill: str = "#000"
    font_size: float = 12.0
    font_family: str = "Liberation Sans, Helvetica, Arial, sans-serif"
    anchor: Literal["start", "middle", "end"] = "middle"
    cls: str | None = None


Primitive = PathPrim | CirclePrim | TextPrim


class Layer(StrictModel):
    name: LayerName
    primitives: list[Primitive] = Field(default_factory=list)


class Viewport(StrictModel):
    """A molecule viewport placed in a diagram (after ELK/grid)."""

    id: str | None = None
    x: float = 0.0
    y: float = 0.0
    width: float
    height: float
    layers: list[Layer] = Field(default_factory=list)


class Scene(StrictModel):
    """Full drawable document before SVG/HTML serialization."""

    width: float
    height: float
    viewports: list[Viewport]
    overlays: list[Primitive] = Field(
        default_factory=list,
        description="Document-space primitives (reaction/network arrows) drawn above viewports",
    )
    halo: list[Primitive] = Field(
        default_factory=list,
        description="Single document-space unioned knockout, under molecule ink",
    )
    meta: dict[str, Any] = Field(default_factory=dict)

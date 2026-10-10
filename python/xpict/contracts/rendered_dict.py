"""TypedDict wire forms for Rust ``DocPaint`` / ``Rendered`` JSON rows."""

from __future__ import annotations

from typing import NotRequired, Required, TypedDict

from xpict.contracts.edge_dict import MoleculeInDict
from xpict.contracts.scene_dict import SceneDict


class SvgAtomDict(TypedDict, total=False):
    index: Required[int]
    element: Required[str]
    x: Required[float]
    y: Required[float]
    label: NotRequired[str | None]
    charge: NotRequired[int]


class SvgBondDict(TypedDict, total=False):
    index: Required[int]
    begin: Required[int]
    end: Required[int]
    order: Required[float]
    stereo: NotRequired[str | None]


class DocPaintDict(TypedDict, total=False):
    """Pass-2 / compose row — ``id`` + ``molecule`` + ``scene`` (optional frame)."""

    id: Required[str]
    molecule: Required[MoleculeInDict]
    scene: Required[SceneDict]
    frame_json: NotRequired[str]


class RenderedDict(TypedDict, total=False):
    """Packed depict row — matches ``xpict_core::doc::Rendered`` JSON."""

    id: Required[str]
    width: Required[float]
    height: Required[float]
    scene: Required[SceneDict]
    molecule: Required[MoleculeInDict]
    source: Required[str]
    frame_molblock: Required[str]
    coords: Required[list[SvgAtomDict]]
    svg_coords: Required[list[SvgAtomDict]]
    bonds: Required[list[SvgBondDict]]

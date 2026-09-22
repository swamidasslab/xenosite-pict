"""Drawable helpers — molecule ink is Rust ``depict_molecule`` (JS surface).

Kept: coord normalization, display labels, annotation/caption drawables, and
thin stubs so older imports keep resolving. Bond / label / shade / mark
painting lives in ``xpict-core``.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Sequence
from dataclasses import dataclass, field

from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.scene import Layer, Viewport
from xpict.contracts.spec import (
    AnnotationSpec,
    MarkSpec,
    MoleculeSpec,
    ShadeSpec,
)
from xpict.draw.annotate import render_annotation
from xpict.draw.collision import CollisionGrid
from xpict.draw.colormap import colormap_rgb
from xpict.draw.drawn import Drawn, Halo
from xpict.draw.label_place import PlacedLabel
from xpict.draw.markush import ring_attachment_annotations
from xpict.draw.metrics import COLLISION_CELL_PX, FONT_PX, PAD_PX, coord_scale
from xpict.draw.mol_title import LabelPack
from xpict.draw.text_metrics import text_box

LAYER_ORDER = ("shading", "halo", "bonds", "labels", "marks", "overlay")
_PAD = PAD_PX


def normalize_coords(
    layout: MoleculeLayout,
    *,
    scale: float | None = None,
) -> tuple[list[tuple[float, float]], float, float]:
    """Return SVG coords (Y-flipped), width, height."""
    if not layout.atoms:
        return [], _PAD * 2, _PAD * 2
    if scale is None:
        scale = coord_scale(layout)
    xs = [a.x for a in layout.atoms]
    ys = [a.y for a in layout.atoms]
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)
    coords = [
        ((a.x - min_x) * scale + _PAD, (max_y - a.y) * scale + _PAD)
        for a in layout.atoms
    ]
    width = (max_x - min_x) * scale + 2 * _PAD
    height = (max_y - min_y) * scale + 2 * _PAD
    return coords, max(width, 2 * _PAD), max(height, 2 * _PAD)


@dataclass
class MolContext:
    """Shared molecule drawing state for annotation / caption overlays."""

    layout: MoleculeLayout
    coords: list[tuple[float, float]]
    texts: list[str | None]
    atom_pos: dict[int, int]
    width: float
    height: float
    grid: CollisionGrid
    layers: dict[str, Layer]
    halo_enabled: bool = True
    label_pack: LabelPack | None = None
    placed_bonds: list[tuple[float, float, float, float]] = field(default_factory=list)
    placed_labels: list[PlacedLabel | None] = field(default_factory=list)
    halo: Halo = field(default_factory=Halo)

    def emit(self, drawn: Drawn) -> None:
        from xpict.draw.drawn import emit_drawn

        emit_drawn(drawn, self.layers, self.halo if self.halo_enabled else None)

    def to_viewport(self) -> Viewport:
        return Viewport(
            width=self.width,
            height=self.height,
            layers=[self.layers[n] for n in LAYER_ORDER if n in self.layers],
        )


def display_text(atom) -> str | None:
    """Label string for an atom (CX / R-group / element)."""
    return atom.label


def mol_occupancy(
    layout: MoleculeLayout,
    coords: Sequence[tuple[float, float]],
    texts: Sequence[str | None],
) -> CollisionGrid:
    grid = CollisionGrid(cell=COLLISION_CELL_PX)
    for (x, y), text in zip(coords, texts, strict=False):
        if text:
            box = text_box(x, y, text, FONT_PX)
            grid.mark_box(*box)
        else:
            grid.mark_circle(x, y, PAD_PX * 0.25)
    return grid


def _shade_rgb(
    z: float, *, colormap: str = "xenosite", diverging: bool = False
) -> str:
    return colormap_rgb(z, name=colormap, diverging=diverging)


def _normalize_shade_scores(zs: list[float], vmin: float, vmax: float) -> list[float]:
    if vmin >= 0 and vmax > 0:
        return [max(0.0, min(1.0, z / vmax)) for z in zs]
    if vmax <= 0 and vmin < 0:
        return [max(-1.0, min(0.0, z / abs(vmin))) for z in zs]
    scale = max(abs(vmin), abs(vmax), 1e-9)
    return [max(-1.0, min(1.0, z / scale)) for z in zs]


class Drawable(ABC):
    @abstractmethod
    def draw(self, ctx: MolContext) -> Drawn | None: ...


@dataclass
class ShadeDrawable(Drawable):
    """Shading disks — Rust ``depict_molecule`` (``atom_shade`` / ``bond_shade``)."""

    spec: ShadeSpec

    def draw(self, ctx: MolContext) -> Drawn | None:
        return None


@dataclass
class BondsDrawable(Drawable):
    """Bond strokes — Rust ``depict_molecule``."""

    color: str = "#111"

    def draw(self, ctx: MolContext) -> Drawn | None:
        return None


@dataclass
class AtomLabelsDrawable(Drawable):
    """Atom labels — Rust ``depict_molecule``."""

    color: str = "#111"

    def draw(self, ctx: MolContext) -> Drawn | None:
        return None


@dataclass
class MarkDrawable(Drawable):
    """Publication marks — Rust ``depict_molecule`` (``mark_atoms`` / ``mark_bonds``)."""

    spec: MarkSpec

    def draw(self, ctx: MolContext) -> Drawn | None:
        return None


@dataclass
class CaptionDrawable(Drawable):
    """Molecule caption (Python overlay; not on JS MVP surface)."""

    color: str = "#111"

    def draw(self, ctx: MolContext) -> Drawn | None:
        from xpict.contracts.scene import TextPrim

        pack = ctx.label_pack
        if pack is None or not pack.text:
            return None
        drawn = Drawn(layer="overlay", halo_cls="halo label-halo")
        drawn.primitives.append(
            TextPrim(
                x=pack.x,
                y=pack.y,
                text=pack.text,
                font_size=pack.font_size,
                fill=self.color,
                anchor=pack.anchor,
                cls="mol-label",
            )
        )
        return drawn


@dataclass
class AnnotationDrawable(Drawable):
    """Callout / region annotation (Python overlay)."""

    spec: AnnotationSpec

    def draw(self, ctx: MolContext) -> Drawn | None:
        return render_annotation(
            self.spec,
            atom_pos=ctx.atom_pos,
            coords=ctx.coords,
            grid=ctx.grid,
            stamp=False,
        )


def molecule_drawables(mol_spec: MoleculeSpec) -> list[Drawable]:
    """Overlays only — ink is Rust. Annotations / captions when present."""
    out: list[Drawable] = []
    for ann in ring_attachment_annotations(mol_spec):
        out.append(AnnotationDrawable(ann))
    for ann in mol_spec.annotations:
        out.append(AnnotationDrawable(ann))
    out.append(CaptionDrawable(color=mol_spec.color or "#111"))
    return out


def paint_molecule(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec,
    *,
    halo: bool = True,
    scale: float | None = None,
) -> tuple[Viewport, Halo]:
    """Paint via Rust ``depict_molecule`` (JS surface)."""
    from xpict.draw.scene_builder import molecule_to_viewport

    vp = molecule_to_viewport(layout, mol_spec, halo=halo, scale=scale)
    return vp, Halo()


__all__ = [
    "AnnotationDrawable",
    "AtomLabelsDrawable",
    "BondsDrawable",
    "CaptionDrawable",
    "Drawable",
    "LAYER_ORDER",
    "MarkDrawable",
    "MolContext",
    "ShadeDrawable",
    "display_text",
    "mol_occupancy",
    "molecule_drawables",
    "normalize_coords",
    "paint_molecule",
    "_shade_rgb",
]

"""Drawable objects: spec → Drawn, with processing owned by each type."""

from __future__ import annotations

import math
from abc import ABC, abstractmethod
from collections.abc import Sequence
from dataclasses import dataclass

from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.scene import CirclePrim, Layer, PathPrim, TextPrim, Viewport
from xpict.contracts.spec import (
    AnnotationSpec,
    MarkKind,
    MarkSpec,
    MoleculeSpec,
    ShadeSpec,
)
from xpict.draw.annotate import render_annotation
from xpict.draw.bonds import DrawnBond, bond_strokes, join_centered_multibonds, shorten
from xpict.draw.collision import CollisionGrid
from xpict.draw.colormap import colormap_rgb
from xpict.draw.drawn import Drawn, emit_drawn, shift_layers
from xpict.draw.glyphs import compile_text_shapes
from xpict.draw.halo import (
    capsule_shape,
    circle_ring_shape,
    disk_shape,
)
from xpict.draw.markush import apply_rgroup_texts, ring_attachment_annotations
from xpict.draw.metrics import (
    BOND_PX,
    COLLISION_CELL_PX,
    FONT_PX,
    HALO_STROKE,
    LABEL_GAP_PX,
    MARK_FRAC,
    OFFSET_PX,
    PAD_PX,
    RADICAL_BASE,
    RADICAL_BASE_BARE,
    RADICAL_DOT_R,
    SHADE_FRAC,
    STROKE_PX,
    coord_scale,
    label_clearance,
)
from xpict.draw.mol_title import LabelPack, pack_label
from xpict.draw.paths import hull_path_d, ink_from_path_prim, polyline_d
from xpict.draw.plotdot import PlotDot
from xpict.draw.rings import bond_interior_normals, find_sssr
from xpict.draw.text_metrics import label_baseline_offset, text_box

# xenopict drawer groups: shading → mol_halo → lines → text → overlay.
# Halo must sit above shade so white knockouts cut channels for bonds.
LAYER_ORDER = ("shading", "halo", "bonds", "labels", "marks", "overlay")
_PAD = PAD_PX


def normalize_coords(
    layout: MoleculeLayout,
) -> tuple[list[tuple[float, float]], float, float]:
    """Return SVG coords (Y-flipped), width, height."""
    if not layout.atoms:
        return [], _PAD * 2, _PAD * 2
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
    """Shared molecule drawing state — coords, occupancy, layers."""

    layout: MoleculeLayout
    coords: list[tuple[float, float]]
    texts: list[str | None]
    atom_pos: dict[int, int]
    width: float
    height: float
    grid: CollisionGrid
    layers: dict[str, Layer]
    halo: bool = True
    label_pack: LabelPack | None = None

    def points(self, atoms: Sequence[int] | None) -> list[tuple[float, float]]:
        if not atoms:
            return []
        return [
            self.coords[self.atom_pos[a]]
            for a in atoms
            if a in self.atom_pos
        ]

    def emit(self, drawn: Drawn | None) -> None:
        if drawn is None:
            return
        emit_drawn(self.layers, drawn, halo=self.halo)
        for box in drawn.boxes:
            self.grid.mark_box(*box, pad=LABEL_GAP_PX * 0.5)

    def grow_to_boxes(self, boxes: Sequence[tuple[float, float, float, float]]) -> None:
        if not boxes:
            return
        min_x = min(b[0] for b in boxes)
        min_y = min(b[1] for b in boxes)
        max_x = max(b[2] for b in boxes)
        max_y = max(b[3] for b in boxes)
        pad = PAD_PX * 0.25
        dx = max(0.0, pad - min_x)
        dy = max(0.0, pad - min_y)
        if dx or dy:
            shift_layers(self.layers, dx, dy)
            if self.label_pack is not None:
                from dataclasses import replace

                self.label_pack = replace(
                    self.label_pack,
                    x=self.label_pack.x + dx,
                    y=self.label_pack.y + dy,
                )
            self.width += dx
            self.height += dy
            max_x += dx
            max_y += dy
        self.width = max(self.width, max_x + pad)
        self.height = max(self.height, max_y + pad)

    def to_viewport(self) -> Viewport:
        return Viewport(
            id=self.layout.id,
            width=self.width,
            height=self.height,
            layers=[self.layers[n] for n in LAYER_ORDER],  # type: ignore[index]
        )


class Drawable(ABC):
    """Spec-backed drawable: owns how it turns into :class:`Drawn`."""

    @abstractmethod
    def draw(self, ctx: MolContext) -> Drawn | None: ...


def display_text(atom) -> str | None:
    """Label string actually painted (includes charge suffix)."""
    label = atom.label
    if label is None and (atom.charge or atom.radical or atom.element == "*"):
        label = "*" if atom.element == "*" else atom.element
    if not label:
        return None
    text = label
    if atom.charge:
        sign = "+" if atom.charge > 0 else "−"
        mag = abs(atom.charge)
        text = f"{label}{sign}" if mag == 1 else f"{label}{mag}{sign}"
    return text


def mol_occupancy(
    layout: MoleculeLayout,
    coords: list[tuple[float, float]],
    texts: list[str | None],
) -> CollisionGrid:
    """Coarse ink occupancy (bonds + atom labels) for packing / callouts."""
    grid = CollisionGrid(cell=COLLISION_CELL_PX)
    atom_pos = {a.index: i for i, a in enumerate(layout.atoms)}
    radius = STROKE_PX + 0.5 * OFFSET_PX
    for bond in layout.bonds:
        i0, i1 = atom_pos.get(bond.begin), atom_pos.get(bond.end)
        if i0 is None or i1 is None:
            continue
        x1, y1 = coords[i0]
        x2, y2 = coords[i1]
        grid.mark_segment(x1, y1, x2, y2, radius=radius)
    for i, text in enumerate(texts):
        x, y = coords[i]
        if text:
            ly = y + label_baseline_offset(FONT_PX)
            box = text_box(
                text, x, ly, font_size=FONT_PX, which="ink", pad=LABEL_GAP_PX * 0.25
            )
            grid.mark_box(*box.as_tuple())
        else:
            grid.mark_circle(x, y, STROKE_PX)
    return grid


def _shade_rgb(
    z: float, *, colormap: str = "xenosite", diverging: bool = False
) -> str:
    """xenopict ``color_map`` → CSS rgb (default LUT: xenosite)."""
    return colormap_rgb(z, name=colormap, diverging=diverging)


def _normalize_shade_scores(zs: list[float], vmin: float, vmax: float) -> list[float]:
    if vmin >= 0 and vmax > 0:
        return [max(0.0, min(1.0, z / vmax)) for z in zs]
    if vmax <= 0 and vmin < 0:
        return [max(-1.0, min(0.0, z / abs(vmin))) for z in zs]
    scale = max(abs(vmin), abs(vmax), 1e-9)
    return [max(-1.0, min(1.0, z / scale)) for z in zs]


@dataclass
class ShadeDrawable(Drawable):
    spec: ShadeSpec

    def draw(self, ctx: MolContext) -> Drawn | None:
        atom_zs = list(self.spec.atoms) if self.spec.atoms else []
        bond_zs = list(self.spec.bonds) if self.spec.bonds else []
        if not atom_zs and not bond_zs:
            return None

        samples: list[float] = []
        if atom_zs:
            samples.extend(atom_zs)
        if bond_zs:
            samples.extend(bond_zs)
        vmin = self.spec.vmin if self.spec.vmin is not None else min(samples)
        vmax = self.spec.vmax if self.spec.vmax is not None else max(samples)
        # xenopict ``diverging_cmap``: map [-1,1]→[0,1] so negatives aren't clipped
        # to the white end of a sequential LUT.
        diverging = vmin < 0.0 < vmax
        # xenopict ``shade()``: scale*0.9, or scale*0.8 when atoms and bonds both set.
        base_r = BOND_PX * (0.8 if atom_zs and bond_zs else SHADE_FRAC)
        drawn = Drawn(layer="shading", halo_cls="halo")
        cmap = self.spec.colormap

        def _emit(zs: list[float], coords: list[tuple[float, float]]) -> None:
            norm = _normalize_shade_scores(zs, vmin, vmax)
            for radius_frac, color_z, (x, y) in PlotDot()(norm, coords[: len(norm)]):
                if abs(color_z) < 0.05 and radius_frac < 0.35:
                    continue
                if abs(color_z) < 0.02:
                    continue
                drawn.primitives.append(
                    CirclePrim(
                        cx=x,
                        cy=y,
                        r=base_r * radius_frac,
                        fill=_shade_rgb(color_z, colormap=cmap, diverging=diverging),
                        opacity=1.0,
                        cls="shade",
                    )
                )

        if atom_zs and ctx.coords:
            _emit(atom_zs, list(ctx.coords))
        if bond_zs and ctx.layout.bonds:
            mids: list[tuple[float, float]] = []
            scores: list[float] = []
            for bond, z in zip(ctx.layout.bonds, bond_zs, strict=False):
                i0, i1 = ctx.atom_pos.get(bond.begin), ctx.atom_pos.get(bond.end)
                if i0 is None or i1 is None:
                    continue
                x1, y1 = ctx.coords[i0]
                x2, y2 = ctx.coords[i1]
                mids.append(((x1 + x2) * 0.5, (y1 + y2) * 0.5))
                scores.append(z)
            if scores:
                _emit(scores, mids)
        return drawn if drawn.primitives else None


@dataclass
class BondsDrawable(Drawable):
    """Bond strokes for the molecule (color from molecule spec)."""

    color: str = "#111"

    def draw(self, ctx: MolContext) -> Drawn | None:
        rings = find_sssr(ctx.layout)
        coords_by_index = {
            a.index: ctx.coords[i] for i, a in enumerate(ctx.layout.atoms)
        }
        normals = (
            bond_interior_normals(rings, coords_by_index) if rings else {}
        )
        prepared: list[DrawnBond] = []
        for bond in ctx.layout.bonds:
            i0, i1 = ctx.atom_pos.get(bond.begin), ctx.atom_pos.get(bond.end)
            if i0 is None or i1 is None:
                continue
            x1, y1 = ctx.coords[i0]
            x2, y2 = ctx.coords[i1]
            g1 = label_clearance(ctx.texts[i0]) if ctx.texts[i0] else 0.0
            g2 = label_clearance(ctx.texts[i1]) if ctx.texts[i1] else 0.0
            x1, y1, x2, y2 = shorten(x1, y1, x2, y2, g1, g2)
            key = (
                (bond.begin, bond.end)
                if bond.begin < bond.end
                else (bond.end, bond.begin)
            )
            prepared.append(
                DrawnBond(
                    index=bond.index,
                    begin=bond.begin,
                    end=bond.end,
                    x1=x1,
                    y1=y1,
                    x2=x2,
                    y2=y2,
                    order=bond.order,
                    interior=normals.get(key),
                    stereo=bond.stereo,
                    begin_labeled=bool(ctx.texts[i0]),
                    end_labeled=bool(ctx.texts[i1]),
                )
            )
        join_centered_multibonds(prepared)
        drawn = Drawn(layer="bonds", halo_cls="halo")
        for bond in prepared:
            strokes = bond_strokes(
                bond.x1,
                bond.y1,
                bond.x2,
                bond.y2,
                bond.order,
                interior=bond.interior,
                stereo=bond.stereo,
                trims=bond.trims,
            )
            tag = f"bond-{bond.index} atom-{bond.begin} atom-{bond.end}"
            parts: list[PathPrim] = []
            if strokes.skeleton is not None:
                strokes.skeleton.stroke = self.color
                strokes.skeleton.cls = f"{tag} bond-skeleton"
                parts.append(strokes.skeleton)
            for p in strokes.offsets:
                p.stroke = self.color
                p.cls = f"{tag} bond-offset"
                parts.append(p)
            for p in strokes.stereo:
                p.stroke = self.color
                if p.fill and p.fill not in ("none", None):
                    p.fill = self.color
                p.cls = f"{tag} {p.cls or 'bond-stereo'}"
                parts.append(p)
            for p in parts:
                drawn.primitives.append(p)
                ink = ink_from_path_prim(p)
                if ink is None:
                    continue
                ink_r = max(p.stroke_width, STROKE_PX) * 0.5
                dist = max(LABEL_GAP_PX, 0.5 * HALO_STROKE - ink_r)
                drawn.ink.append(ink)
                drawn.ink_dists.append(dist)
        return drawn if drawn.primitives else None


@dataclass
class AtomLabelsDrawable(Drawable):
    """Heteroatom labels and radical dots."""

    def draw(self, ctx: MolContext) -> Drawn | None:
        drawn = Drawn(layer="labels", halo_cls="halo label-halo")
        for i, atom in enumerate(ctx.layout.atoms):
            label = ctx.texts[i]
            x, y = ctx.coords[i]
            if atom.radical > 0:
                nbr_angs = []
                for bond in ctx.layout.bonds:
                    if bond.begin == atom.index or bond.end == atom.index:
                        other = bond.end if bond.begin == atom.index else bond.begin
                        oi = ctx.atom_pos.get(other)
                        if oi is not None:
                            nbr_angs.append(
                                math.atan2(
                                    ctx.coords[oi][1] - y, ctx.coords[oi][0] - x
                                )
                            )
                if nbr_angs:
                    nbr_angs.sort()
                    best_mid, best_w = nbr_angs[0] + math.pi, 0.0
                    for j, a0 in enumerate(nbr_angs):
                        a1 = nbr_angs[(j + 1) % len(nbr_angs)]
                        w = a1 - a0
                        if w <= 0:
                            w += 2 * math.pi
                        if w > best_w:
                            best_w = w
                            best_mid = a0 + w / 2
                    ang = best_mid
                else:
                    ang = -math.pi / 2
                base = RADICAL_BASE if label else RADICAL_BASE_BARE
                n = min(atom.radical, 3)
                for k in range(n):
                    spread = (k - (n - 1) / 2) * 0.35
                    dang = ang + spread
                    cx = x + math.cos(dang) * base
                    cy = y + math.sin(dang) * base
                    drawn.primitives.append(
                        CirclePrim(
                            cx=cx,
                            cy=cy,
                            r=RADICAL_DOT_R,
                            fill="#111",
                            stroke="none",
                            opacity=1.0,
                            cls=f"atom-{atom.index} radical",
                        )
                    )
                    ink = disk_shape(cx, cy, RADICAL_DOT_R)
                    if ink is not None:
                        drawn.ink.append(ink)
                        drawn.ink_dists.append(LABEL_GAP_PX)
            if not label:
                continue
            label_y = y + label_baseline_offset(FONT_PX)
            drawn.primitives.append(
                TextPrim(
                    x=x,
                    y=label_y,
                    text=label,
                    font_size=FONT_PX,
                    cls=f"atom-{atom.index} label",
                )
            )
            ink = compile_text_shapes(label, x, label_y, font_size=FONT_PX)
            if ink is not None:
                drawn.ink.append(ink)
                drawn.ink_dists.append(LABEL_GAP_PX)
        return drawn if drawn.primitives else None


@dataclass
class MarkDrawable(Drawable):
    """Legacy publication marks (circles / bond strokes / hull)."""

    spec: MarkSpec

    def draw(self, ctx: MolContext) -> Drawn | None:
        color = self.spec.color or "#c44"
        drawn = Drawn(layer="marks", halo_cls="halo mark-halo")
        mark = self.spec
        if mark.kind == MarkKind.substructure and mark.atoms:
            pts = ctx.points(mark.atoms)
            path = hull_path_d(pts, pad=BOND_PX * 0.45)
            if path:
                prim = PathPrim(
                    d=path,
                    stroke=color,
                    fill=color,
                    stroke_width=1.5,
                    opacity=0.25,
                    cls="substructure-mark",
                )
                drawn.primitives.append(prim)
                ink = ink_from_path_prim(prim)
                if ink is not None:
                    drawn.ink.append(ink)
                    drawn.ink_dists.append(LABEL_GAP_PX)
            return drawn if drawn.primitives else None
        if mark.atoms:
            r = BOND_PX * MARK_FRAC
            for ai in mark.atoms:
                pos = ctx.atom_pos.get(ai)
                if pos is None:
                    continue
                x, y = ctx.coords[pos]
                drawn.primitives.append(
                    CirclePrim(
                        cx=x,
                        cy=y,
                        r=r,
                        fill="none",
                        stroke=color,
                        stroke_width=STROKE_PX,
                        opacity=0.85,
                        cls=f"atom-{ai} mark",
                    )
                )
                ink = circle_ring_shape(x, y, r, STROKE_PX)
                if ink is not None:
                    drawn.ink.append(ink)
                    drawn.ink_dists.append(LABEL_GAP_PX)
        if mark.bonds:
            for a, b in mark.bonds:
                ia, ib = ctx.atom_pos.get(a), ctx.atom_pos.get(b)
                if ia is None or ib is None:
                    continue
                x1, y1 = ctx.coords[ia]
                x2, y2 = ctx.coords[ib]
                drawn.primitives.append(
                    PathPrim(
                        d=polyline_d([(x1, y1), (x2, y2)]),
                        stroke=color,
                        stroke_width=HALO_STROKE,
                        opacity=0.35,
                        cls=f"bond-mark atom-{a} atom-{b}",
                    )
                )
                ink = capsule_shape(x1, y1, x2, y2, 0.5 * HALO_STROKE)
                if ink is not None:
                    drawn.ink.append(ink)
                    drawn.ink_dists.append(LABEL_GAP_PX)
        return drawn if drawn.primitives else None


@dataclass
class CaptionDrawable(Drawable):
    """Molecule caption already packed into ``ctx.label_pack``."""

    def draw(self, ctx: MolContext) -> Drawn | None:
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
                fill="#222",
                anchor=pack.anchor,
                cls="mol-label",
            )
        )
        ink = compile_text_shapes(
            pack.text, pack.x, pack.y, font_size=pack.font_size, anchor=pack.anchor
        )
        if ink is not None:
            drawn.ink.append(ink)
            drawn.ink_dists.append(LABEL_GAP_PX)
        return drawn


@dataclass
class AnnotationDrawable(Drawable):
    """Callout / region annotation from :class:`AnnotationSpec`."""

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
    """Ordered drawables for one molecule (shade → bonds → labels → marks → annots → caption)."""
    out: list[Drawable] = []
    if mol_spec.shade is not None:
        out.append(ShadeDrawable(mol_spec.shade))
    out.append(BondsDrawable(color=mol_spec.color or "#111"))
    out.append(AtomLabelsDrawable())
    for mark in mol_spec.marks:
        out.append(MarkDrawable(mark))
    for ann in ring_attachment_annotations(mol_spec):
        out.append(AnnotationDrawable(ann))
    for ann in mol_spec.annotations:
        out.append(AnnotationDrawable(ann))
    out.append(CaptionDrawable())
    return out


def paint_molecule(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec,
    *,
    halo: bool = True,
) -> Viewport:
    """Build a molecule viewport by emitting drawables into shared layers."""
    coords, width, height = normalize_coords(layout)
    texts = apply_rgroup_texts(
        layout, mol_spec, [display_text(a) for a in layout.atoms]
    )
    label_pack = None
    label_text = None
    if mol_spec.label is not None:
        label_text = mol_spec.label.text.strip() or None
    if label_text is not None and mol_spec.label is not None:
        occ = mol_occupancy(layout, coords, texts)
        label_pack = pack_label(
            frame_width=width,
            frame_height=height,
            occupancy=occ,
            text=label_text,
            pos=mol_spec.label.pos,
        )
        if label_pack.dx or label_pack.dy:
            coords = [(x + label_pack.dx, y + label_pack.dy) for x, y in coords]
        width, height = label_pack.width, label_pack.height

    ctx = MolContext(
        layout=layout,
        coords=coords,
        texts=texts,
        atom_pos={a.index: i for i, a in enumerate(layout.atoms)},
        width=width,
        height=height,
        grid=mol_occupancy(layout, coords, texts),
        layers={name: Layer(name=name) for name in LAYER_ORDER},  # type: ignore[arg-type]
        halo=halo,
        label_pack=label_pack,
    )
    annot_boxes: list[tuple[float, float, float, float]] = []
    for drawable in molecule_drawables(mol_spec):
        drawn = drawable.draw(ctx)
        if drawn is None:
            continue
        if isinstance(drawable, AnnotationDrawable):
            annot_boxes.extend(drawn.boxes)
        ctx.emit(drawn)
    ctx.grow_to_boxes(annot_boxes)
    vp = ctx.to_viewport()
    return vp.model_copy(update={"id": layout.id or mol_spec.id})


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
]

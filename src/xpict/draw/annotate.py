"""Molecule annotations as :class:`~xpict.draw.drawable.Drawable` objects."""

from __future__ import annotations

import math
from collections.abc import Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from xpict.contracts.scene import CirclePrim, PathPrim, Primitive, TextPrim
from xpict.contracts.spec import AnnotKind, AnnotationSpec, AnnotPrefer
from xpict.draw.collision import CollisionGrid
from xpict.draw.drawn import Drawn
from xpict.draw.glyphs import compile_text_shapes
from xpict.draw.halo import (
    capsule_shape,
    circle_ring_shape,
    disk_shape,
    path_polyline_shape,
)
from xpict.draw.metrics import (
    ANNOT_ARROW_PX,
    ANNOT_FONT_PX,
    ANNOT_GAP_PX,
    ANNOT_PAD_PX,
    ANNOT_STROKE_PX,
    LABEL_GAP_PX,
)
from xpict.draw.paths import filled_arrow_head_d, oval_d, polyline_d, rect_d
from xpict.draw.text_metrics import measure_text, text_box
from xpict.native_bridge import Shape

if TYPE_CHECKING:
    from xpict.draw.drawable import Drawable, MolContext
else:
    from abc import ABC, abstractmethod

    class Drawable(ABC):  # noqa: B903 — runtime base when not importing drawable
        @abstractmethod
        def draw(self, ctx: Any) -> Drawn | None: ...


# Back-compat alias used by tests / callers.
AnnotDraw = Drawn


def _atom_points(
    atoms: Sequence[int] | None,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
) -> list[tuple[float, float]]:
    if not atoms:
        return []
    return [coords[atom_pos[a]] for a in atoms if a in atom_pos]


def _centroid(pts: Sequence[tuple[float, float]]) -> tuple[float, float]:
    return (
        sum(p[0] for p in pts) / len(pts),
        sum(p[1] for p in pts) / len(pts),
    )


def _target_point(
    ann: AnnotationSpec,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
) -> tuple[float, float] | None:
    if ann.ring:
        pts = _atom_points(ann.ring, atom_pos, coords)
        if pts:
            return _centroid(pts)
    if ann.bonds:
        mids: list[tuple[float, float]] = []
        for a, b in ann.bonds:
            ia, ib = atom_pos.get(a), atom_pos.get(b)
            if ia is None or ib is None:
                continue
            x1, y1 = coords[ia]
            x2, y2 = coords[ib]
            mids.append(((x1 + x2) * 0.5, (y1 + y2) * 0.5))
        if mids:
            return _centroid(mids)
    if ann.atoms:
        pts = _atom_points(ann.atoms, atom_pos, coords)
        if pts:
            return _centroid(pts)
    return None


def _bounds(
    pts: Sequence[tuple[float, float]], pad: float
) -> tuple[float, float, float, float] | None:
    if not pts:
        return None
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad


def _spline_path(pts: Sequence[tuple[float, float]], pad: float) -> str | None:
    if not pts:
        return None
    if len(pts) == 1:
        x, y = pts[0]
        return oval_d(x, y, pad, pad)
    geom = Shape.multipoint_buffer(list(pts), pad)
    if geom.is_empty:
        return None
    polys = geom.polygons()
    if not polys:
        return None
    # Largest exterior by rough bbox area.
    exterior = max(
        polys,
        key=lambda ph: (max(p[0] for p in ph[0]) - min(p[0] for p in ph[0]))
        * (max(p[1] for p in ph[0]) - min(p[1] for p in ph[0])),
    )[0]
    ring = list(exterior)
    if len(ring) > 1 and ring[0] == ring[-1]:
        ring = ring[:-1]
    return polyline_d(ring, closed=True)


def _arrow_drawn(
    tip_x: float,
    tip_y: float,
    from_x: float,
    from_y: float,
    *,
    color: str,
    size: float = ANNOT_ARROW_PX,
    stroke: float = ANNOT_STROKE_PX,
) -> Drawn:
    dx, dy = tip_x - from_x, tip_y - from_y
    length = math.hypot(dx, dy)
    if length < 1e-6:
        return Drawn()
    ux, uy = dx / length, dy / length
    tip = (tip_x - ux * ANNOT_GAP_PX * 0.35, tip_y - uy * ANNOT_GAP_PX * 0.35)
    base = (tip[0] - ux * size, tip[1] - uy * size)
    start = (from_x, from_y)
    drawn = Drawn(layer="marks", halo=True, halo_cls="halo annot-halo")
    if math.hypot(base[0] - start[0], base[1] - start[1]) >= size * 0.5:
        drawn.primitives.append(
            PathPrim(
                d=polyline_d([start, base]),
                stroke=color,
                fill="none",
                stroke_width=stroke,
                cls="annot-arrow-shaft",
            )
        )
        ink = capsule_shape(start[0], start[1], base[0], base[1], 0.5 * stroke)
        if ink is not None:
            drawn.ink.append(ink)
    drawn.primitives.append(
        PathPrim(
            d=filled_arrow_head_d(tip[0], tip[1], ux, uy, size=size),
            stroke=color,
            fill=color,
            stroke_width=1.0,
            cls="annot-arrow-head",
        )
    )
    ink = disk_shape(tip[0], tip[1], size * 0.45)
    if ink is not None:
        drawn.ink.append(ink)
    return drawn


def _draw_region(ann: AnnotationSpec, pts: Sequence[tuple[float, float]], *, color: str) -> Drawn | None:
    pad = ANNOT_PAD_PX
    stroke = ANNOT_STROKE_PX
    drawn = Drawn(layer="marks", halo=True, halo_cls="halo annot-halo")
    if ann.kind == AnnotKind.box:
        bounds = _bounds(pts, pad)
        if bounds is None:
            return None
        xmin, ymin, xmax, ymax = bounds
        drawn.primitives.append(
            PathPrim(
                d=rect_d(xmin, ymin, xmax, ymax),
                stroke=color,
                fill="none",
                stroke_width=stroke,
                opacity=0.9,
                cls="annot-box",
            )
        )
        ink = path_polyline_shape(
            [(xmin, ymin), (xmax, ymin), (xmax, ymax), (xmin, ymax), (xmin, ymin)],
            0.5 * stroke,
        )
        if ink is not None:
            drawn.ink.append(ink)
        drawn.boxes.append((xmin, ymin, xmax, ymax))
        return drawn

    if ann.kind == AnnotKind.oval:
        bounds = _bounds(pts, pad)
        if bounds is None:
            return None
        xmin, ymin, xmax, ymax = bounds
        cx, cy = (xmin + xmax) * 0.5, (ymin + ymax) * 0.5
        rx, ry = (xmax - xmin) * 0.5, (ymax - ymin) * 0.5
        if len(pts) == 1:
            rx = ry = max(rx, ry, pad)
        drawn.primitives.append(
            PathPrim(
                d=oval_d(cx, cy, rx, ry),
                stroke=color,
                fill="none",
                stroke_width=stroke,
                opacity=0.9,
                cls="annot-oval",
            )
        )
        ink = circle_ring_shape(cx, cy, 0.5 * (rx + ry), stroke)
        if ink is not None:
            drawn.ink.append(ink)
        drawn.boxes.append((cx - rx, cy - ry, cx + rx, cy + ry))
        return drawn

    if ann.kind == AnnotKind.spline:
        d = _spline_path(pts, pad)
        if not d:
            return None
        drawn.primitives.append(
            PathPrim(
                d=d,
                stroke=color,
                fill="none",
                stroke_width=stroke,
                opacity=0.9,
                cls="annot-spline",
            )
        )
        geom = Shape.multipoint_buffer(list(pts), pad)
        outer = geom.buffer(0.5 * stroke)
        inner = geom.buffer(-0.5 * stroke) if not geom.is_empty else None
        ink = outer.difference(inner) if inner is not None and not inner.is_empty else outer
        if ink is not None and not ink.is_empty:
            drawn.ink.append(ink)
        if geom.bounds is not None:
            drawn.boxes.append(tuple(geom.bounds))
        return drawn
    return None


def _draw_callout(
    ann: AnnotationSpec,
    target: tuple[float, float],
    grid: CollisionGrid,
    *,
    color: str,
) -> Drawn:
    ax, ay = target
    drawn = Drawn(layer="marks", halo=True, halo_cls="halo annot-halo")
    label = (ann.label or "").strip()
    side = ann.prefer.value if isinstance(ann.prefer, AnnotPrefer) else str(ann.prefer)

    if label:
        metrics = measure_text(label, ANNOT_FONT_PX)
        tw = max(metrics.advance, metrics.typo.width)
        th = metrics.typo.height
        cx, cy, placed_side = grid.find_slot(
            ax, ay, tw, th, prefer=side, gap=ANNOT_GAP_PX, pad=LABEL_GAP_PX * 0.5
        )
        baseline = cy - metrics.typo.cy
        drawn.primitives.append(
            TextPrim(
                x=cx,
                y=baseline,
                text=label,
                font_size=ANNOT_FONT_PX,
                fill=color,
                anchor="middle",
                cls="annot-label",
            )
        )
        box = text_box(
            label, cx, baseline, font_size=ANNOT_FONT_PX, anchor="middle", which="typo"
        )
        drawn.boxes.append(box.as_tuple())
        glyph = compile_text_shapes(
            label, cx, baseline, font_size=ANNOT_FONT_PX, anchor="middle"
        )
        if glyph is not None:
            drawn.ink.append(glyph)
        if ann.arrow:
            if placed_side == "right":
                lx, ly = box.xmin, cy
            elif placed_side == "left":
                lx, ly = box.xmax, cy
            elif placed_side == "top":
                lx, ly = cx, box.ymax
            else:
                lx, ly = cx, box.ymin
            drawn.extend(_arrow_drawn(ax, ay, lx, ly, color=color))
    elif ann.arrow:
        stub = ANNOT_GAP_PX + ANNOT_ARROW_PX
        if side in ("", "auto"):
            side = "right"
        offsets = {
            "right": (ax + stub, ay),
            "left": (ax - stub, ay),
            "top": (ax, ay - stub),
            "bottom": (ax, ay + stub),
        }
        fx, fy = offsets.get(side, (ax + stub, ay))
        drawn.extend(_arrow_drawn(ax, ay, fx, fy, color=color))
        drawn.boxes.append(
            (min(ax, fx) - 2, min(ay, fy) - 2, max(ax, fx) + 2, max(ay, fy) + 2)
        )
    else:
        r = ANNOT_PAD_PX * 0.55
        drawn.primitives.append(
            CirclePrim(
                cx=ax,
                cy=ay,
                r=r,
                fill="none",
                stroke=color,
                stroke_width=ANNOT_STROKE_PX,
                opacity=0.9,
                cls="annot-mark",
            )
        )
        ring = circle_ring_shape(ax, ay, r, ANNOT_STROKE_PX)
        if ring is not None:
            drawn.ink.append(ring)
        drawn.boxes.append((ax - r, ay - r, ax + r, ay + r))
    return drawn


def render_annotation(
    ann: AnnotationSpec,
    *,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
    grid: CollisionGrid,
    stamp: bool = True,
) -> Drawn | None:
    """Render one annotation. When ``stamp``, mark boxes on ``grid``."""
    color = ann.color or "#c44"
    if ann.kind in (AnnotKind.box, AnnotKind.oval, AnnotKind.spline):
        pts = _atom_points(ann.atoms, atom_pos, coords)
        if not pts:
            return None
        drawn = _draw_region(ann, pts, color=color)
        if drawn is None:
            return None
        if ann.label and ann.label.strip():
            tx, ty = _centroid(pts)
            for box in drawn.boxes:
                grid.mark_box(*box, pad=LABEL_GAP_PX)
            call = AnnotationSpec(
                kind=AnnotKind.callout,
                atoms=ann.atoms,
                label=ann.label,
                color=color,
                arrow=False,
                prefer=ann.prefer,
            )
            drawn.extend(_draw_callout(call, (tx, ty), grid, color=color))
        if stamp:
            for box in drawn.boxes:
                grid.mark_box(*box, pad=LABEL_GAP_PX * 0.5)
        return drawn

    target = _target_point(ann, atom_pos, coords)
    if target is None:
        return None
    drawn = _draw_callout(ann, target, grid, color=color)
    if stamp:
        for box in drawn.boxes:
            grid.mark_box(*box, pad=LABEL_GAP_PX * 0.5)
    return drawn


@dataclass
class AnnotationDrawable(Drawable):
    """Callout / region annotation from :class:`AnnotationSpec`."""

    spec: AnnotationSpec

    def draw(self, ctx: Any) -> Drawn | None:
        return render_annotation(
            self.spec,
            atom_pos=ctx.atom_pos,
            coords=ctx.coords,
            grid=ctx.grid,
            stamp=False,  # MolContext.emit stamps boxes
        )


def draw_annotation(
    ann: AnnotationSpec,
    *,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
    grid: CollisionGrid,
) -> Drawn | None:
    return render_annotation(
        ann, atom_pos=atom_pos, coords=coords, grid=grid, stamp=True
    )


def draw_annotations(
    annotations: Sequence[AnnotationSpec],
    *,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
    grid: CollisionGrid,
) -> Drawn:
    out = Drawn(layer="marks", halo=True, halo_cls="halo annot-halo")
    for ann in annotations:
        drawn = draw_annotation(
            ann, atom_pos=atom_pos, coords=coords, grid=grid
        )
        out.extend(drawn)
    return out


__all__ = [
    "AnnotDraw",
    "AnnotationDrawable",
    "draw_annotation",
    "draw_annotations",
    "render_annotation",
]

"""Molecule annotations: callouts, region shapes, collision-aware labels.

Uses the shared :class:`~xenosite.pict.draw.collision.CollisionGrid` so
callout captions land in free air next to atoms, bonds, or ring centers.
"""

from __future__ import annotations

import math
from collections.abc import Sequence
from dataclasses import dataclass

from shapely.geometry import MultiPoint

from xenosite.pict.contracts.scene import CirclePrim, PathPrim, Primitive, TextPrim
from xenosite.pict.contracts.spec import AnnotKind, AnnotationSpec, AnnotPrefer
from xenosite.pict.draw.collision import CollisionGrid
from xenosite.pict.draw.glyphs import compile_text_shapes
from xenosite.pict.draw.halo import (
    capsule_shape,
    circle_ring_shape,
    disk_shape,
    path_polyline_shape,
)
from xenosite.pict.draw.metrics import (
    ANNOT_ARROW_PX,
    ANNOT_FONT_PX,
    ANNOT_GAP_PX,
    ANNOT_PAD_PX,
    ANNOT_STROKE_PX,
    LABEL_GAP_PX,
)
from xenosite.pict.draw.text_metrics import measure_text, text_box


@dataclass(frozen=True)
class AnnotDraw:
    """Drawn annotation primitives plus ink for haloing / occupancy."""

    primitives: list[Primitive]
    ink: list  # shapely geometries
    boxes: list[tuple[float, float, float, float]]  # stamp into collision grid


def _atom_points(
    atoms: Sequence[int] | None,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
) -> list[tuple[float, float]]:
    if not atoms:
        return []
    out: list[tuple[float, float]] = []
    for ai in atoms:
        idx = atom_pos.get(ai)
        if idx is not None:
            out.append(coords[idx])
    return out


def _target_point(
    ann: AnnotationSpec,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
) -> tuple[float, float] | None:
    """Callout / label anchor: atom, bond midpoint, or ring centroid."""
    if ann.ring:
        pts = _atom_points(ann.ring, atom_pos, coords)
        if pts:
            return (
                sum(p[0] for p in pts) / len(pts),
                sum(p[1] for p in pts) / len(pts),
            )
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
            return (
                sum(p[0] for p in mids) / len(mids),
                sum(p[1] for p in mids) / len(mids),
            )
    if ann.atoms:
        pts = _atom_points(ann.atoms, atom_pos, coords)
        if pts:
            return (
                sum(p[0] for p in pts) / len(pts),
                sum(p[1] for p in pts) / len(pts),
            )
    return None


def _bounds(
    pts: Sequence[tuple[float, float]], pad: float
) -> tuple[float, float, float, float] | None:
    if not pts:
        return None
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad


def _rect_path(xmin: float, ymin: float, xmax: float, ymax: float) -> str:
    return (
        f"M {xmin:.2f} {ymin:.2f} L {xmax:.2f} {ymin:.2f} "
        f"L {xmax:.2f} {ymax:.2f} L {xmin:.2f} {ymax:.2f} Z"
    )


def _oval_path(cx: float, cy: float, rx: float, ry: float) -> str:
    return (
        f"M {cx - rx:.2f} {cy:.2f} "
        f"A {rx:.2f} {ry:.2f} 0 1 0 {cx + rx:.2f} {cy:.2f} "
        f"A {rx:.2f} {ry:.2f} 0 1 0 {cx - rx:.2f} {cy:.2f}"
    )


def _coords_to_path(pts: Sequence[tuple[float, float]], *, closed: bool = True) -> str:
    if not pts:
        return ""
    bits = [f"M {pts[0][0]:.2f} {pts[0][1]:.2f}"]
    for x, y in pts[1:]:
        bits.append(f"L {x:.2f} {y:.2f}")
    if closed:
        bits.append("Z")
    return " ".join(bits)


def _spline_path(pts: Sequence[tuple[float, float]], pad: float) -> str | None:
    """Smooth closed outline: shapely buffer of the atom set."""
    if not pts:
        return None
    if len(pts) == 1:
        x, y = pts[0]
        return _oval_path(x, y, pad, pad)
    geom = MultiPoint(pts).buffer(pad, quad_segs=8)
    if geom.is_empty:
        return None
    poly = geom if geom.geom_type == "Polygon" else max(geom.geoms, key=lambda g: g.area)
    ring = list(poly.exterior.coords)
    if len(ring) > 1 and ring[0] == ring[-1]:
        ring = ring[:-1]
    return _coords_to_path(ring, closed=True)


def _arrow_prims(
    tip_x: float,
    tip_y: float,
    from_x: float,
    from_y: float,
    *,
    color: str,
    size: float = ANNOT_ARROW_PX,
    stroke: float = ANNOT_STROKE_PX,
) -> tuple[list[Primitive], list]:
    """Shaft + filled head pointing at ``(tip_x, tip_y)`` from the label."""
    dx, dy = tip_x - from_x, tip_y - from_y
    length = math.hypot(dx, dy)
    if length < 1e-6:
        return [], []
    ux, uy = dx / length, dy / length
    # Hold the tip slightly off the target so it does not cover the atom.
    tip = (tip_x - ux * ANNOT_GAP_PX * 0.35, tip_y - uy * ANNOT_GAP_PX * 0.35)
    # Trim shaft so it meets the head base.
    base = (tip[0] - ux * size, tip[1] - uy * size)
    # Start shaft just outside the label box center toward the tip.
    start = (from_x + ux * 0.0, from_y + uy * 0.0)
    shaft_len = math.hypot(base[0] - start[0], base[1] - start[1])
    if shaft_len < size * 0.5:
        # Label too close — head only.
        shaft_d = None
    else:
        shaft_d = f"M {start[0]:.2f} {start[1]:.2f} L {base[0]:.2f} {base[1]:.2f}"

    px, py = -uy, ux
    half = size * 0.45
    head_d = (
        f"M {tip[0]:.2f} {tip[1]:.2f} "
        f"L {base[0] + px * half:.2f} {base[1] + py * half:.2f} "
        f"L {base[0] - px * half:.2f} {base[1] - py * half:.2f} Z"
    )
    prims: list[Primitive] = []
    ink: list = []
    if shaft_d:
        prims.append(
            PathPrim(
                d=shaft_d,
                stroke=color,
                fill="none",
                stroke_width=stroke,
                cls="annot-arrow-shaft",
            )
        )
        ink.append(capsule_shape(start[0], start[1], base[0], base[1], 0.5 * stroke))
    prims.append(
        PathPrim(
            d=head_d,
            stroke=color,
            fill=color,
            stroke_width=1.0,
            cls="annot-arrow-head",
        )
    )
    ink.append(disk_shape(tip[0], tip[1], size * 0.45))
    return prims, [g for g in ink if g is not None]


def _draw_region(
    ann: AnnotationSpec,
    pts: Sequence[tuple[float, float]],
    *,
    color: str,
) -> AnnotDraw | None:
    pad = ANNOT_PAD_PX
    stroke = ANNOT_STROKE_PX
    if ann.kind == AnnotKind.box:
        bounds = _bounds(pts, pad)
        if bounds is None:
            return None
        xmin, ymin, xmax, ymax = bounds
        d = _rect_path(xmin, ymin, xmax, ymax)
        prim = PathPrim(
            d=d,
            stroke=color,
            fill="none",
            stroke_width=stroke,
            opacity=0.9,
            cls="annot-box",
        )
        ring_pts = [
            (xmin, ymin),
            (xmax, ymin),
            (xmax, ymax),
            (xmin, ymax),
            (xmin, ymin),
        ]
        ink = path_polyline_shape(ring_pts, 0.5 * stroke)
        return AnnotDraw(
            primitives=[prim],
            ink=[ink] if ink is not None else [],
            boxes=[(xmin, ymin, xmax, ymax)],
        )

    if ann.kind == AnnotKind.oval:
        bounds = _bounds(pts, pad)
        if bounds is None:
            return None
        xmin, ymin, xmax, ymax = bounds
        cx, cy = (xmin + xmax) * 0.5, (ymin + ymax) * 0.5
        rx, ry = (xmax - xmin) * 0.5, (ymax - ymin) * 0.5
        # Single-atom oval stays circular.
        if len(pts) == 1:
            rx = ry = max(rx, ry, pad)
        d = _oval_path(cx, cy, rx, ry)
        prim = PathPrim(
            d=d,
            stroke=color,
            fill="none",
            stroke_width=stroke,
            opacity=0.9,
            cls="annot-oval",
        )
        # Approximate annular ink with a circle ring at the mean radius.
        r_mean = 0.5 * (rx + ry)
        ink = circle_ring_shape(cx, cy, r_mean, stroke)
        return AnnotDraw(
            primitives=[prim],
            ink=[ink] if ink is not None else [],
            boxes=[(cx - rx, cy - ry, cx + rx, cy + ry)],
        )

    if ann.kind == AnnotKind.spline:
        d = _spline_path(pts, pad)
        if not d:
            return None
        prim = PathPrim(
            d=d,
            stroke=color,
            fill="none",
            stroke_width=stroke,
            opacity=0.9,
            cls="annot-spline",
        )
        geom = MultiPoint(pts).buffer(pad, quad_segs=8)
        # Stroke-like ink: outer buffer minus shrunk interior.
        outer = geom.buffer(0.5 * stroke, quad_segs=8)
        inner = geom.buffer(-0.5 * stroke, quad_segs=8) if not geom.is_empty else None
        ink = outer.difference(inner) if inner is not None and not inner.is_empty else outer
        minx, miny, maxx, maxy = geom.bounds
        return AnnotDraw(
            primitives=[prim],
            ink=[ink] if ink is not None and not ink.is_empty else [],
            boxes=[(minx, miny, maxx, maxy)],
        )
    return None


def _draw_callout(
    ann: AnnotationSpec,
    target: tuple[float, float],
    grid: CollisionGrid,
    *,
    color: str,
) -> AnnotDraw:
    ax, ay = target
    prims: list[Primitive] = []
    ink: list = []
    boxes: list[tuple[float, float, float, float]] = []

    label = (ann.label or "").strip()
    side = ann.prefer.value if isinstance(ann.prefer, AnnotPrefer) else str(ann.prefer)

    if label:
        metrics = measure_text(label, ANNOT_FONT_PX)
        tw = max(metrics.advance, metrics.typo.width)
        th = metrics.typo.height
        cx, cy, placed_side = grid.find_slot(
            ax,
            ay,
            tw,
            th,
            prefer=side,
            gap=ANNOT_GAP_PX,
            pad=LABEL_GAP_PX * 0.5,
        )
        # Vertically center typographic box on ``cy``.
        baseline = cy - metrics.typo.cy
        text_prim = TextPrim(
            x=cx,
            y=baseline,
            text=label,
            font_size=ANNOT_FONT_PX,
            fill=color,
            anchor="middle",
            cls="annot-label",
        )
        prims.append(text_prim)
        box = text_box(
            label, cx, baseline, font_size=ANNOT_FONT_PX, anchor="middle", which="typo"
        )
        boxes.append(box.as_tuple())
        glyph = compile_text_shapes(
            label, cx, baseline, font_size=ANNOT_FONT_PX, anchor="middle"
        )
        if glyph is not None:
            ink.append(glyph)

        if ann.arrow:
            # Arrow from near label toward the target.
            # Pick a point on the label box edge facing the target.
            lx, ly = cx, cy
            if placed_side == "right":
                lx = box.xmin
                ly = cy
            elif placed_side == "left":
                lx = box.xmax
                ly = cy
            elif placed_side == "top":
                lx = cx
                ly = box.ymax
            else:
                lx = cx
                ly = box.ymin
            arrow_prims, arrow_ink = _arrow_prims(ax, ay, lx, ly, color=color)
            prims.extend(arrow_prims)
            ink.extend(arrow_ink)
    elif ann.arrow:
        # Indicator only: short stub arrow pointing at the target from prefer side.
        stub = ANNOT_GAP_PX + ANNOT_ARROW_PX
        if side in ("", "auto"):
            side = "right"
        if side == "right":
            fx, fy = ax + stub, ay
        elif side == "left":
            fx, fy = ax - stub, ay
        elif side == "top":
            fx, fy = ax, ay - stub
        else:
            fx, fy = ax, ay + stub
        arrow_prims, arrow_ink = _arrow_prims(ax, ay, fx, fy, color=color)
        prims.extend(arrow_prims)
        ink.extend(arrow_ink)
        boxes.append(
            (min(ax, fx) - 2, min(ay, fy) - 2, max(ax, fx) + 2, max(ay, fy) + 2)
        )
    else:
        # Bare highlight: small open circle on the target.
        r = ANNOT_PAD_PX * 0.55
        prims.append(
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
            ink.append(ring)
        boxes.append((ax - r, ay - r, ax + r, ay + r))

    return AnnotDraw(primitives=prims, ink=ink, boxes=boxes)


def draw_annotation(
    ann: AnnotationSpec,
    *,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
    grid: CollisionGrid,
) -> AnnotDraw | None:
    """Render one annotation; stamps free slots into ``grid`` when placed."""
    color = ann.color or "#c44"

    if ann.kind in (AnnotKind.box, AnnotKind.oval, AnnotKind.spline):
        pts = _atom_points(ann.atoms, atom_pos, coords)
        if not pts:
            return None
        drawn = _draw_region(ann, pts, color=color)
        if drawn is None:
            return None
        # Optional caption near region centroid.
        if ann.label and ann.label.strip():
            tx = sum(p[0] for p in pts) / len(pts)
            ty = sum(p[1] for p in pts) / len(pts)
            call = AnnotationSpec(
                kind=AnnotKind.callout,
                atoms=ann.atoms,
                label=ann.label,
                color=color,
                arrow=False,
                prefer=ann.prefer,
            )
            # Temporarily mark region so the caption avoids it.
            for box in drawn.boxes:
                grid.mark_box(*box, pad=LABEL_GAP_PX)
            extra = _draw_callout(call, (tx, ty), grid, color=color)
            drawn = AnnotDraw(
                primitives=[*drawn.primitives, *extra.primitives],
                ink=[*drawn.ink, *extra.ink],
                boxes=[*drawn.boxes, *extra.boxes],
            )
        for box in drawn.boxes:
            grid.mark_box(*box, pad=LABEL_GAP_PX * 0.5)
        return drawn

    # callout (default)
    target = _target_point(ann, atom_pos, coords)
    if target is None:
        return None
    drawn = _draw_callout(ann, target, grid, color=color)
    for box in drawn.boxes:
        grid.mark_box(*box, pad=LABEL_GAP_PX * 0.5)
    return drawn


def draw_annotations(
    annotations: Sequence[AnnotationSpec],
    *,
    atom_pos: dict[int, int],
    coords: Sequence[tuple[float, float]],
    grid: CollisionGrid,
) -> AnnotDraw:
    """Draw all annotations in order, sharing one collision grid."""
    prims: list[Primitive] = []
    ink: list = []
    boxes: list[tuple[float, float, float, float]] = []
    for ann in annotations:
        drawn = draw_annotation(
            ann, atom_pos=atom_pos, coords=coords, grid=grid
        )
        if drawn is None:
            continue
        prims.extend(drawn.primitives)
        ink.extend(drawn.ink)
        boxes.extend(drawn.boxes)
    return AnnotDraw(primitives=prims, ink=ink, boxes=boxes)


__all__ = [
    "AnnotDraw",
    "draw_annotation",
    "draw_annotations",
]

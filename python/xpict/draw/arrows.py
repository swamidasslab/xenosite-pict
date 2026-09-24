"""Reaction / network edge arrows in document space (between viewports)."""

from __future__ import annotations

import math
from collections.abc import Sequence

from xpict.contracts.scene import PathPrim, Primitive, TextPrim, Viewport
from xpict.future.spec import EdgeArrow, EdgeSpec

_DEFAULT_COLOR = "#222"
_DEFAULT_WIDTH = 1.6
_HEAD = 9.0
_GAP = 6.0
_DASH = "6 4"
_EQ_SEP = 3.2


def _vp_center(vp: Viewport) -> tuple[float, float]:
    return vp.x + vp.width * 0.5, vp.y + vp.height * 0.5


def _clip_box_edge(
    cx: float, cy: float, tx: float, ty: float, w: float, h: float, pad: float
) -> tuple[float, float]:
    """Point on the padded axis-aligned box around (cx,cy) toward (tx,ty)."""
    dx, dy = tx - cx, ty - cy
    if abs(dx) < 1e-9 and abs(dy) < 1e-9:
        return cx, cy
    hw, hh = w * 0.5 + pad, h * 0.5 + pad
    # Scale so the ray hits the rectangle boundary.
    sx = hw / abs(dx) if abs(dx) > 1e-9 else float("inf")
    sy = hh / abs(dy) if abs(dy) > 1e-9 else float("inf")
    t = min(sx, sy)
    return cx + dx * t, cy + dy * t


def edge_anchors(
    src: Viewport, tgt: Viewport, *, pad: float = _GAP
) -> tuple[tuple[float, float], tuple[float, float]]:
    """Viewport-boundary anchors for an edge from ``src`` to ``tgt``."""
    sx, sy = _vp_center(src)
    tx, ty = _vp_center(tgt)
    p0 = _clip_box_edge(sx, sy, tx, ty, src.width, src.height, pad)
    p1 = _clip_box_edge(tx, ty, sx, sy, tgt.width, tgt.height, pad)
    return p0, p1


def _unit(dx: float, dy: float) -> tuple[float, float]:
    L = math.hypot(dx, dy) or 1.0
    return dx / L, dy / L


def _perp(ux: float, uy: float) -> tuple[float, float]:
    return -uy, ux


def _polyline_length(pts: Sequence[tuple[float, float]]) -> float:
    total = 0.0
    for i in range(1, len(pts)):
        total += math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1])
    return total


def _shorten_polyline_end(
    pts: Sequence[tuple[float, float]], amount: float
) -> tuple[list[tuple[float, float]], tuple[float, float], tuple[float, float]]:
    """Trim ``amount`` from the end. Returns (remaining, tip, unit_dir_into_tip)."""
    if len(pts) < 2:
        p = pts[0] if pts else (0.0, 0.0)
        return [p], p, (1.0, 0.0)
    pts = list(pts)
    tip = pts[-1]
    remaining = amount
    while len(pts) >= 2 and remaining > 0:
        x1, y1 = pts[-2]
        x2, y2 = pts[-1]
        seg = math.hypot(x2 - x1, y2 - y1)
        if seg <= 1e-9:
            pts.pop()
            continue
        if seg > remaining:
            ux, uy = _unit(x2 - x1, y2 - y1)
            nx = x2 - ux * remaining
            ny = y2 - uy * remaining
            pts[-1] = (nx, ny)
            return pts, tip, (ux, uy)
        remaining -= seg
        pts.pop()
    ux, uy = (1.0, 0.0)
    if len(pts) >= 2:
        ux, uy = _unit(pts[-1][0] - pts[-2][0], pts[-1][1] - pts[-2][1])
    elif len(pts) == 1:
        # Degenerate — keep tip direction from original last segment if possible.
        pass
    return pts, tip, (ux, uy)


def _path_d(pts: Sequence[tuple[float, float]]) -> str:
    from xpict.draw.paths import polyline_d

    return polyline_d(pts)


def _offset_polyline(pts: Sequence[tuple[float, float]], dist: float) -> list[tuple[float, float]]:
    """Simple parallel offset (good for orthogonal ELK routes)."""
    if len(pts) < 2:
        return list(pts)
    out: list[tuple[float, float]] = []
    n = len(pts)
    for i, (x, y) in enumerate(pts):
        if i == 0:
            ux, uy = _unit(pts[1][0] - x, pts[1][1] - y)
            px, py = _perp(ux, uy)
        elif i == n - 1:
            ux, uy = _unit(x - pts[i - 1][0], y - pts[i - 1][1])
            px, py = _perp(ux, uy)
        else:
            # Average adjacent segment normals (stable on axis-aligned bends).
            u1x, u1y = _unit(x - pts[i - 1][0], y - pts[i - 1][1])
            u2x, u2y = _unit(pts[i + 1][0] - x, pts[i + 1][1] - y)
            p1x, p1y = _perp(u1x, u1y)
            p2x, p2y = _perp(u2x, u2y)
            px, py = p1x + p2x, p1y + p2y
            L = math.hypot(px, py) or 1.0
            px, py = px / L, py / L
        out.append((x + px * dist, y + py * dist))
    return out


def _shaft_poly(
    pts: Sequence[tuple[float, float]],
    *,
    color: str,
    width: float,
    dashed: bool,
    cls: str,
) -> PathPrim:
    return PathPrim(
        d=_path_d(pts),
        stroke=color,
        fill="none",
        stroke_width=width,
        stroke_dasharray=_DASH if dashed else None,
        cls=cls,
    )


def _filled_head(
    tip_x: float,
    tip_y: float,
    ux: float,
    uy: float,
    *,
    color: str,
    size: float,
    cls: str,
) -> PathPrim:
    """Filled triangle — fill only; a stroke would grow past the tip."""
    from xpict.draw.paths import filled_arrow_head_d

    return PathPrim(
        d=filled_arrow_head_d(tip_x, tip_y, ux, uy, size=size),
        stroke="none",
        fill=color,
        stroke_width=0.0,
        cls=cls,
    )


def _open_head(
    tip_x: float,
    tip_y: float,
    ux: float,
    uy: float,
    *,
    color: str,
    width: float,
    size: float,
    cls: str,
) -> PathPrim:
    px, py = _perp(ux, uy)
    bx, by = tip_x - ux * size, tip_y - uy * size
    half = size * 0.55
    return PathPrim(
        d=(
            f"M {bx + px * half:.2f} {by + py * half:.2f} "
            f"L {tip_x:.2f} {tip_y:.2f} "
            f"L {bx - px * half:.2f} {by - py * half:.2f}"
        ),
        stroke=color,
        fill="none",
        stroke_width=width,
        cls=cls,
    )


def _harpoon_poly(
    pts: Sequence[tuple[float, float]],
    *,
    color: str,
    width: float,
    dashed: bool,
    head_size: float,
    cls: str,
) -> list[PathPrim]:
    """Half-arrow along a polyline."""
    shaft, tip, (ux, uy) = _shorten_polyline_end(pts, head_size)
    px, py = _perp(ux, uy)
    barb = head_size * 0.55
    bx, by = tip[0] - ux * head_size, tip[1] - uy * head_size
    out: list[PathPrim] = []
    if len(shaft) >= 2:
        out.append(_shaft_poly(shaft, color=color, width=width, dashed=dashed, cls=cls))
    out.append(
        PathPrim(
            d=(f"M {tip[0]:.2f} {tip[1]:.2f} L {bx + px * barb:.2f} {by + py * barb:.2f}"),
            stroke=color,
            fill="none",
            stroke_width=width,
            cls=f"{cls} harpoon",
        )
    )
    return out


def _label_point(pts: Sequence[tuple[float, float]]) -> tuple[float, float, float, float]:
    """Mid-path point and a local perpendicular for label offset."""
    if len(pts) < 2:
        x, y = pts[0] if pts else (0.0, 0.0)
        return x, y, 0.0, -1.0
    # Place on the longest segment (clearest for orthogonal routes).
    best_i = 0
    best_len = -1.0
    for i in range(1, len(pts)):
        L = math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1])
        if L > best_len:
            best_len = L
            best_i = i
    x1, y1 = pts[best_i - 1]
    x2, y2 = pts[best_i]
    ux, uy = _unit(x2 - x1, y2 - y1)
    px, py = _perp(ux, uy)
    return (x1 + x2) * 0.5, (y1 + y2) * 0.5, px, py


def resolve_route(
    src: Viewport,
    tgt: Viewport,
    route: Sequence[tuple[float, float]] | None = None,
) -> list[tuple[float, float]]:
    """ELK polyline when present; otherwise straight viewport-boundary anchors."""
    if route is not None and len(route) >= 2:
        return [(float(x), float(y)) for x, y in route]
    (x1, y1), (x2, y2) = edge_anchors(src, tgt)
    return [(x1, y1), (x2, y2)]


def edge_primitives(
    edge: EdgeSpec,
    src: Viewport,
    tgt: Viewport,
    *,
    index: int = 0,
    route: Sequence[tuple[float, float]] | None = None,
) -> list[Primitive]:
    """Build document-space primitives for one diagram edge."""
    pts = resolve_route(src, tgt, route)
    if _polyline_length(pts) < 4.0:
        return []

    color = edge.color or _DEFAULT_COLOR
    width = edge.stroke_width if edge.stroke_width is not None else _DEFAULT_WIDTH
    dashed = edge.dashed
    arrow = edge.arrow
    cls = f"edge edge-{index}"
    out: list[Primitive] = []

    if arrow == EdgeArrow.equilibrium:
        fwd = _offset_polyline(pts, _EQ_SEP)
        rev = list(reversed(_offset_polyline(pts, -_EQ_SEP)))
        out.extend(
            _harpoon_poly(
                fwd,
                color=color,
                width=width,
                dashed=dashed,
                head_size=_HEAD * 0.85,
                cls=f"{cls} eq-fwd",
            )
        )
        out.extend(
            _harpoon_poly(
                rev,
                color=color,
                width=width,
                dashed=dashed,
                head_size=_HEAD * 0.85,
                cls=f"{cls} eq-rev",
            )
        )
    elif arrow == EdgeArrow.line:
        out.append(_shaft_poly(pts, color=color, width=width, dashed=dashed, cls=cls))
    elif arrow == EdgeArrow.open:
        shaft, tip, (ux, uy) = _shorten_polyline_end(pts, _HEAD)
        if len(shaft) >= 2:
            out.append(_shaft_poly(shaft, color=color, width=width, dashed=dashed, cls=cls))
        out.append(
            _open_head(
                tip[0],
                tip[1],
                ux,
                uy,
                color=color,
                width=width,
                size=_HEAD,
                cls=f"{cls} head",
            )
        )
    else:
        # forward
        shaft, tip, (ux, uy) = _shorten_polyline_end(pts, _HEAD)
        if len(shaft) >= 2:
            out.append(_shaft_poly(shaft, color=color, width=width, dashed=dashed, cls=cls))
        out.append(_filled_head(tip[0], tip[1], ux, uy, color=color, size=_HEAD, cls=f"{cls} head"))

    if edge.label:
        mx, my, px, py = _label_point(pts)
        pos = (getattr(edge, "label_pos", None) or "above").lower()
        # Flip the default "above" offset for below / right.
        if pos in {"below", "right"}:
            px, py = -px, -py
        out.append(
            TextPrim(
                x=mx + px * 10.0,
                y=my + py * 10.0 + 4.0,
                text=edge.label,
                fill=color,
                font_size=11.0,
                anchor="middle",
                cls=f"{cls} label",
            )
        )
    return out


def diagram_overlays(
    edges: Sequence[EdgeSpec],
    viewports: Sequence[Viewport],
    edge_paths: Sequence[Sequence[tuple[float, float]] | None] | None = None,
) -> list[Primitive]:
    """Document overlays for all edges that resolve to placed viewports."""
    by_id = {vp.id: vp for vp in viewports if vp.id}
    prims: list[Primitive] = []
    for i, edge in enumerate(edges):
        src = by_id.get(edge.source)
        tgt = by_id.get(edge.target)
        if src is None or tgt is None:
            continue
        route = None
        if edge_paths is not None and i < len(edge_paths):
            route = edge_paths[i]
        prims.extend(edge_primitives(edge, src, tgt, index=i, route=route))
    return prims

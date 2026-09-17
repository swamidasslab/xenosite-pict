"""Reaction / network edge arrows in document space (between viewports)."""

from __future__ import annotations

import math
from collections.abc import Sequence

from xenosite.pict.contracts.scene import PathPrim, Primitive, TextPrim, Viewport
from xenosite.pict.contracts.spec import EdgeArrow, EdgeSpec

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


def _shaft(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    *,
    color: str,
    width: float,
    dashed: bool,
    cls: str,
) -> PathPrim:
    return PathPrim(
        d=f"M {x1:.2f} {y1:.2f} L {x2:.2f} {y2:.2f}",
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
    px, py = _perp(ux, uy)
    bx, by = tip_x - ux * size, tip_y - uy * size
    half = size * 0.45
    return PathPrim(
        d=(
            f"M {tip_x:.2f} {tip_y:.2f} "
            f"L {bx + px * half:.2f} {by + py * half:.2f} "
            f"L {bx - px * half:.2f} {by - py * half:.2f} Z"
        ),
        stroke=color,
        fill=color,
        stroke_width=1.0,
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


def _harpoon(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    *,
    color: str,
    width: float,
    dashed: bool,
    head_size: float,
    cls: str,
) -> list[PathPrim]:
    """Single half-arrow (shaft + one-sided barb) from (x1,y1) → (x2,y2)."""
    ux, uy = _unit(x2 - x1, y2 - y1)
    px, py = _perp(ux, uy)
    tip_x, tip_y = x2, y2
    sx2 = tip_x - ux * head_size
    sy2 = tip_y - uy * head_size
    barb = head_size * 0.55
    return [
        _shaft(x1, y1, sx2, sy2, color=color, width=width, dashed=dashed, cls=cls),
        PathPrim(
            d=(
                f"M {tip_x:.2f} {tip_y:.2f} "
                f"L {sx2 + px * barb:.2f} {sy2 + py * barb:.2f}"
            ),
            stroke=color,
            fill="none",
            stroke_width=width,
            cls=f"{cls} harpoon",
        ),
    ]


def edge_primitives(
    edge: EdgeSpec,
    src: Viewport,
    tgt: Viewport,
    *,
    index: int = 0,
) -> list[Primitive]:
    """Build document-space primitives for one diagram edge."""
    (x1, y1), (x2, y2) = edge_anchors(src, tgt)
    dx, dy = x2 - x1, y2 - y1
    if math.hypot(dx, dy) < 4.0:
        return []

    color = edge.color or _DEFAULT_COLOR
    width = edge.stroke_width if edge.stroke_width is not None else _DEFAULT_WIDTH
    dashed = edge.dashed
    arrow = edge.arrow
    cls = f"edge edge-{index}"
    out: list[Primitive] = []

    ux, uy = _unit(dx, dy)
    px, py = _perp(ux, uy)

    if arrow == EdgeArrow.equilibrium:
        # Two parallel half-arrows (⇌).
        ox, oy = px * _EQ_SEP, py * _EQ_SEP
        out.extend(
            _harpoon(
                x1 + ox,
                y1 + oy,
                x2 + ox,
                y2 + oy,
                color=color,
                width=width,
                dashed=dashed,
                head_size=_HEAD * 0.85,
                cls=f"{cls} eq-fwd",
            )
        )
        out.extend(
            _harpoon(
                x2 - ox,
                y2 - oy,
                x1 - ox,
                y1 - oy,
                color=color,
                width=width,
                dashed=dashed,
                head_size=_HEAD * 0.85,
                cls=f"{cls} eq-rev",
            )
        )
    elif arrow == EdgeArrow.line:
        out.append(
            _shaft(x1, y1, x2, y2, color=color, width=width, dashed=dashed, cls=cls)
        )
    elif arrow == EdgeArrow.open:
        # Shorten shaft for hollow head.
        tip_x, tip_y = x2, y2
        sx2 = tip_x - ux * _HEAD
        sy2 = tip_y - uy * _HEAD
        out.append(
            _shaft(x1, y1, sx2, sy2, color=color, width=width, dashed=dashed, cls=cls)
        )
        out.append(
            _open_head(
                tip_x, tip_y, ux, uy, color=color, width=width, size=_HEAD, cls=f"{cls} head"
            )
        )
    else:
        # forward
        tip_x, tip_y = x2, y2
        sx2 = tip_x - ux * _HEAD
        sy2 = tip_y - uy * _HEAD
        out.append(
            _shaft(x1, y1, sx2, sy2, color=color, width=width, dashed=dashed, cls=cls)
        )
        out.append(
            _filled_head(tip_x, tip_y, ux, uy, color=color, size=_HEAD, cls=f"{cls} head")
        )

    if edge.label:
        mx = (x1 + x2) * 0.5 + px * 10.0
        my = (y1 + y2) * 0.5 + py * 10.0
        out.append(
            TextPrim(
                x=mx,
                y=my + 4.0,
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
) -> list[Primitive]:
    """Document overlays for all edges that resolve to placed viewports."""
    by_id = {vp.id: vp for vp in viewports if vp.id}
    prims: list[Primitive] = []
    for i, edge in enumerate(edges):
        src = by_id.get(edge.source)
        tgt = by_id.get(edge.target)
        if src is None or tgt is None:
            continue
        prims.extend(edge_primitives(edge, src, tgt, index=i))
    return prims

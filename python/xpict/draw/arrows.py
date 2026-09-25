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
# Flatten tiny orthogonal/polyline jogs; larger → more aggressive straightening.
_KINK_PX = 10.0
# Corner fillet radius for non-orthogonal (polyline) bends.
_TURN_RADIUS = 36.0


def _vp_center(vp: Viewport) -> tuple[float, float]:
    return vp.x + vp.width * 0.5, vp.y + vp.height * 0.5


def _routing_name(routing: object | None) -> str | None:
    if routing is None:
        return None
    raw = getattr(routing, "value", routing)
    return str(raw).lower()


def _axis_aligned(pts: Sequence[tuple[float, float]], *, tol: float = 1e-3) -> bool:
    """True when every segment is horizontal or vertical (orthogonal route)."""
    if len(pts) < 2:
        return True
    for i in range(1, len(pts)):
        dx = abs(pts[i][0] - pts[i - 1][0])
        dy = abs(pts[i][1] - pts[i - 1][1])
        if dx > tol and dy > tol:
            return False
    return True


def _should_fillet(
    pts: Sequence[tuple[float, float]],
    *,
    edge_routing: object | None = None,
) -> bool:
    """Fillet soft polyline turns; keep orthogonal shafts axis-aligned."""
    name = _routing_name(edge_routing)
    if name == "orthogonal":
        return False
    if name in {"polyline", "splines"}:
        # Explicit polyline may still be axis-aligned from ELK — keep H/V sharp.
        return not _axis_aligned(pts)
    # Unknown / unset: fillet only when the path already has diagonal segments.
    return len(pts) > 2 and not _axis_aligned(pts)


def _point_line_distance(
    px: float, py: float, ax: float, ay: float, bx: float, by: float
) -> float:
    """Perpendicular distance from P to infinite line AB."""
    dx, dy = bx - ax, by - ay
    L2 = dx * dx + dy * dy
    if L2 < 1e-12:
        return math.hypot(px - ax, py - ay)
    t = ((px - ax) * dx + (py - ay) * dy) / L2
    qx, qy = ax + t * dx, ay + t * dy
    return math.hypot(px - qx, py - qy)


def simplify_route(
    pts: Sequence[tuple[float, float]],
    *,
    kink_px: float = _KINK_PX,
) -> list[tuple[float, float]]:
    """Drop near-duplicates and flatten kinks under ``kink_px``.

    Used for orthogonal / polyline routes so a tiny jog becomes a straight
    shaft. Spline-dense paths also shed noise but keep real bends.
    """
    if len(pts) < 2:
        return [(float(x), float(y)) for x, y in pts]
    # 1) Dedup consecutive points.
    cleaned: list[tuple[float, float]] = []
    for x, y in pts:
        p = (float(x), float(y))
        if not cleaned or math.hypot(p[0] - cleaned[-1][0], p[1] - cleaned[-1][1]) > 1e-6:
            cleaned.append(p)
    if len(cleaned) <= 2:
        return cleaned

    # 2) Collapse short middle segments (classic orthogonal micro-jog).
    changed = True
    while changed and len(cleaned) > 2:
        changed = False
        out: list[tuple[float, float]] = [cleaned[0]]
        i = 1
        while i < len(cleaned) - 1:
            x0, y0 = out[-1]
            x1, y1 = cleaned[i]
            x2, y2 = cleaned[i + 1]
            seg = math.hypot(x1 - x0, y1 - y0)
            nxt = math.hypot(x2 - x1, y2 - y1)
            # Short elbow: skip the middle vertex and keep walking.
            if seg < kink_px or nxt < kink_px:
                i += 1
                changed = True
                continue
            out.append(cleaned[i])
            i += 1
        out.append(cleaned[-1])
        cleaned = out

    # 3) Remove vertices nearly collinear with neighbors (RDP-lite pass).
    changed = True
    while changed and len(cleaned) > 2:
        changed = False
        out = [cleaned[0]]
        for i in range(1, len(cleaned) - 1):
            ax, ay = out[-1]
            px, py = cleaned[i]
            bx, by = cleaned[i + 1]
            if _point_line_distance(px, py, ax, ay, bx, by) < kink_px:
                changed = True
                continue
            out.append(cleaned[i])
        out.append(cleaned[-1])
        cleaned = out

    return cleaned


def filleted_path_d(
    pts: Sequence[tuple[float, float]],
    *,
    radius: float = _TURN_RADIUS,
) -> str:
    """Polyline with quadratic fillets at corners (larger ``radius`` → softer turns)."""
    if len(pts) < 2:
        return ""
    if len(pts) == 2 or radius <= 0:
        return _path_d(pts)

    bits: list[str] = [f"M {pts[0][0]:.2f} {pts[0][1]:.2f}"]
    for i in range(1, len(pts) - 1):
        ax, ay = pts[i - 1]
        bx, by = pts[i]
        cx, cy = pts[i + 1]
        v1x, v1y = ax - bx, ay - by
        v2x, v2y = cx - bx, cy - by
        len1 = math.hypot(v1x, v1y) or 1.0
        len2 = math.hypot(v2x, v2y) or 1.0
        # Skip near-straight corners — already simplified, but be safe.
        dot = (v1x * v2x + v1y * v2y) / (len1 * len2)
        if dot < -0.98:  # ~180° — almost straight through
            bits.append(f"L {bx:.2f} {by:.2f}")
            continue
        r = min(radius, 0.45 * len1, 0.45 * len2)
        if r < 1.0:
            bits.append(f"L {bx:.2f} {by:.2f}")
            continue
        u1x, u1y = v1x / len1, v1y / len1
        u2x, u2y = v2x / len2, v2y / len2
        p1 = (bx + u1x * r, by + u1y * r)
        p2 = (bx + u2x * r, by + u2y * r)
        bits.append(f"L {p1[0]:.2f} {p1[1]:.2f}")
        bits.append(f"Q {bx:.2f} {by:.2f} {p2[0]:.2f} {p2[1]:.2f}")
    bits.append(f"L {pts[-1][0]:.2f} {pts[-1][1]:.2f}")
    return " ".join(bits)


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
    fillet: bool = True,
) -> PathPrim:
    d = filleted_path_d(pts) if fillet else _path_d(pts)
    return PathPrim(
        d=d,
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
    fillet: bool = True,
) -> list[PathPrim]:
    """Half-arrow along a polyline."""
    shaft, tip, (ux, uy) = _shorten_polyline_end(pts, head_size)
    px, py = _perp(ux, uy)
    barb = head_size * 0.55
    bx, by = tip[0] - ux * head_size, tip[1] - uy * head_size
    out: list[PathPrim] = []
    if len(shaft) >= 2:
        out.append(
            _shaft_poly(
                shaft, color=color, width=width, dashed=dashed, cls=cls, fillet=fillet
            )
        )
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
    """ELK polyline when present; otherwise straight viewport-boundary anchors.

    Orthogonal / polyline routes are simplified so micro-kinks under
    ``_KINK_PX`` collapse to a straight shaft before painting.
    """
    if route is not None and len(route) >= 2:
        return simplify_route([(float(x), float(y)) for x, y in route])
    (x1, y1), (x2, y2) = edge_anchors(src, tgt)
    return [(x1, y1), (x2, y2)]


def edge_primitives(
    edge: EdgeSpec,
    src: Viewport,
    tgt: Viewport,
    *,
    index: int = 0,
    route: Sequence[tuple[float, float]] | None = None,
    scheme_routing: object | None = None,
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
    routing = getattr(edge, "edge_routing", None) or scheme_routing
    fillet = _should_fillet(pts, edge_routing=routing)
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
                fillet=fillet,
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
                fillet=fillet,
            )
        )
    elif arrow == EdgeArrow.line:
        out.append(
            _shaft_poly(
                pts, color=color, width=width, dashed=dashed, cls=cls, fillet=fillet
            )
        )
    elif arrow == EdgeArrow.open:
        shaft, tip, (ux, uy) = _shorten_polyline_end(pts, _HEAD)
        if len(shaft) >= 2:
            out.append(
                _shaft_poly(
                    shaft,
                    color=color,
                    width=width,
                    dashed=dashed,
                    cls=cls,
                    fillet=fillet,
                )
            )
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
            out.append(
                _shaft_poly(
                    shaft,
                    color=color,
                    width=width,
                    dashed=dashed,
                    cls=cls,
                    fillet=fillet,
                )
            )
        out.append(
            _filled_head(
                tip[0], tip[1], ux, uy, color=color, size=_HEAD, cls=f"{cls} head"
            )
        )

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
    *,
    scheme_routing: object | None = None,
) -> list[Primitive]:
    """Document overlays for all edges that resolve to placed viewports."""
    by_id = {vp.id: vp for vp in viewports if vp.id}
    prims: list[Primitive] = []
    for i, edge in enumerate(edges):
        # Multi reactant/product: primary shaft uses first endpoints; route from ELK
        # may already span the hyperedge. Additional endpoints share the same overlay
        # until multi-shaft paint lands.
        src = by_id.get(edge.sources[0]) if edge.sources else None
        tgt = by_id.get(edge.targets[0]) if edge.targets else None
        if src is None or tgt is None:
            continue
        route = None
        if edge_paths is not None and i < len(edge_paths):
            route = edge_paths[i]
        prims.extend(
            edge_primitives(
                edge,
                src,
                tgt,
                index=i,
                route=route,
                scheme_routing=scheme_routing,
            )
        )
    return prims

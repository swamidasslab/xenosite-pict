"""Shared SVG path ``d`` builders and stroke→ink helpers."""

from __future__ import annotations

import math
import re
from collections.abc import Sequence

from xpict.contracts.scene import PathPrim
from xpict.draw.halo import path_polyline_shape
from xpict.draw.metrics import STROKE_PX
from xpict.native_bridge import CapsuleInk, Shape


def polyline_d(pts: Sequence[tuple[float, float]], *, closed: bool = False) -> str:
    if not pts:
        return ""
    bits = [f"M {pts[0][0]:.2f} {pts[0][1]:.2f}"]
    for x, y in pts[1:]:
        bits.append(f"L {x:.2f} {y:.2f}")
    if closed:
        bits.append("Z")
    return " ".join(bits)


def rect_d(xmin: float, ymin: float, xmax: float, ymax: float) -> str:
    return polyline_d(
        [(xmin, ymin), (xmax, ymin), (xmax, ymax), (xmin, ymax)], closed=True
    )


def oval_d(cx: float, cy: float, rx: float, ry: float) -> str:
    return (
        f"M {cx - rx:.2f} {cy:.2f} "
        f"A {rx:.2f} {ry:.2f} 0 1 0 {cx + rx:.2f} {cy:.2f} "
        f"A {rx:.2f} {ry:.2f} 0 1 0 {cx - rx:.2f} {cy:.2f}"
    )


def filled_arrow_head_d(
    tip_x: float,
    tip_y: float,
    ux: float,
    uy: float,
    *,
    size: float,
) -> str:
    """Filled triangle pointing along ``(ux, uy)`` with tip at ``(tip_x, tip_y)``."""
    px, py = -uy, ux
    bx, by = tip_x - ux * size, tip_y - uy * size
    half = size * 0.45
    return (
        f"M {tip_x:.2f} {tip_y:.2f} "
        f"L {bx + px * half:.2f} {by + py * half:.2f} "
        f"L {bx - px * half:.2f} {by - py * half:.2f} Z"
    )


def path_coords(d: str) -> list[tuple[float, float]]:
    """Best-effort point list from an SVG path ``d`` (numeric pairs)."""
    nums = [float(x) for x in re.findall(r"[-+]?(?:\d+\.?\d*|\.\d+)", d)]
    return [(nums[i], nums[i + 1]) for i in range(0, len(nums) - 1, 2)]


def shift_path_d(d: str, dx: float, dy: float) -> str:
    """Translate absolute M/L points and A endpoints in a path ``d``."""
    tokens = re.findall(r"[MLZA]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?", d)
    out: list[str] = []
    i = 0
    cmd = "M"
    while i < len(tokens):
        t = tokens[i]
        if t in "MLZA":
            cmd = t
            out.append(t)
            i += 1
            continue
        if cmd in ("M", "L"):
            x = float(t)
            y = float(tokens[i + 1])
            out.append(f"{x + dx:.2f}")
            out.append(f"{y + dy:.2f}")
            i += 2
            continue
        if cmd == "A":
            nums = [float(tokens[j]) for j in range(i, i + 7)]
            nums[5] += dx
            nums[6] += dy
            out.extend(f"{n:.2f}" if j >= 5 else f"{n:g}" for j, n in enumerate(nums))
            i += 7
            continue
        if cmd == "Z":
            i += 1
            continue
        out.append(t)
        i += 1
    return " ".join(out)


def ink_from_path_prim(p: PathPrim) -> Shape | CapsuleInk | None:
    """Approximate a stroked/filled PathPrim as ink for haloing.

    Filled paths (arrow heads, wedges) grow by half the stroke so the ink
    matches the painted footprint — SVG stroke is centered on the fill edge.
    """
    pts = path_coords(p.d)
    if not pts:
        return None
    if p.fill not in (None, "none") and len(pts) >= 3:
        poly = Shape.from_ring(pts)
        if poly.is_empty:
            return None
        sw = float(p.stroke_width or 0.0)
        if p.stroke not in (None, "none") and sw > 0:
            grown = poly.buffer(0.5 * sw)
            if not grown.is_empty:
                return grown
        return poly
    radius = max(p.stroke_width, STROKE_PX) * 0.5
    return path_polyline_shape(pts, radius)


def convex_hull(points: Sequence[tuple[float, float]]) -> list[tuple[float, float]]:
    pts = sorted(set(points))
    if len(pts) <= 2:
        return list(pts)

    def cross(
        o: tuple[float, float], a: tuple[float, float], b: tuple[float, float]
    ) -> float:
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower: list[tuple[float, float]] = []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    upper: list[tuple[float, float]] = []
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return lower[:-1] + upper[:-1]


def hull_path_d(points: Sequence[tuple[float, float]], pad: float = 10.0) -> str | None:
    """Padded convex hull as SVG path ``d`` (circle / capsule / polygon)."""
    if not points:
        return None
    if len(points) == 1:
        x, y = points[0]
        return oval_d(x, y, pad, pad)
    hull = convex_hull(points)
    if len(hull) == 2:
        (x1, y1), (x2, y2) = hull
        dx, dy = x2 - x1, y2 - y1
        length = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / length * pad, dx / length * pad
        return polyline_d(
            [
                (x1 + nx, y1 + ny),
                (x2 + nx, y2 + ny),
                (x2 - nx, y2 - ny),
                (x1 - nx, y1 - ny),
            ],
            closed=True,
        )
    cx = sum(p[0] for p in hull) / len(hull)
    cy = sum(p[1] for p in hull) / len(hull)
    expanded = []
    for x, y in hull:
        dx, dy = x - cx, y - cy
        length = math.hypot(dx, dy) or 1.0
        expanded.append((x + dx / length * pad, y + dy / length * pad))
    return polyline_d(expanded, closed=True)


__all__ = [
    "polyline_d",
    "rect_d",
    "oval_d",
    "filled_arrow_head_d",
    "path_coords",
    "shift_path_d",
    "ink_from_path_prim",
    "convex_hull",
    "hull_path_d",
]

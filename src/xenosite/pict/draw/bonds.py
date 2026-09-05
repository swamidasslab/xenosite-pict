"""Bond drawing: skeleton centerlines, then multi-bond offsets, then stereo.

Follows quality depictors (RDKit MolDraw2D, Indigo render, CDK):

1. Every bond is a single **skeleton** centerline first (connected graph).
2. Double/triple add shortened **offset** strokes (ring doubles toward interior).
3. Tetrahedral stereo replaces the skeleton with solid/hashed wedges
   (thin end at stereocenter — RDKit ``BEGINWEDGE`` / ``BEGINDASH``).
4. ``either`` → wavy single or crossed double.

Do not invent new conventions; match RDKit/Indigo geometry.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

from xenosite.pict.contracts.scene import PathPrim

_STROKE = 1.55
_WEDGE_HALF = 3.2  # half-width at fat end (SVG units)
_HASH_COUNT = 6


def depict_order(order: float) -> float:
    """Normalize engine bond orders for 2D depiction.

    Indigo aromatic = 4; RDKit aromatic = 1.5. Prefer Kekulé from backends;
    this is a safety net so aromatics never become triple lines.
    """
    if order >= 3.5:
        return 1.0
    if 1.4 <= order < 1.6:
        return 1.0
    return order


def shorten(
    x1: float, y1: float, x2: float, y2: float, gap1: float, gap2: float
) -> tuple[float, float, float, float]:
    dx, dy = x2 - x1, y2 - y1
    length = math.hypot(dx, dy) or 1.0
    if gap1 + gap2 >= length:
        mx, my = (x1 + x2) / 2, (y1 + y2) / 2
        return mx, my, mx, my
    ux, uy = dx / length, dy / length
    return x1 + ux * gap1, y1 + uy * gap1, x2 - ux * gap2, y2 - uy * gap2


def _line(
    x1: float, y1: float, x2: float, y2: float, *, cls: str = "bond", width: float = _STROKE
) -> PathPrim:
    return PathPrim(
        d=f"M {x1:.2f} {y1:.2f} L {x2:.2f} {y2:.2f}",
        stroke="#111",
        stroke_width=width,
        cls=cls,
    )


def _unit(x1: float, y1: float, x2: float, y2: float) -> tuple[float, float, float, float, float]:
    dx, dy = x2 - x1, y2 - y1
    length = math.hypot(dx, dy) or 1.0
    return dx / length, dy / length, -dy / length, dx / length, length


@dataclass
class BondStrokes:
    """Strokes for one bond, ordered for painting: skeleton → offsets → stereo."""

    skeleton: PathPrim | None = None
    offsets: list[PathPrim] = field(default_factory=list)
    stereo: list[PathPrim] = field(default_factory=list)

    def paint_order(self) -> list[PathPrim]:
        out: list[PathPrim] = []
        if self.skeleton is not None:
            out.append(self.skeleton)
        out.extend(self.offsets)
        out.extend(self.stereo)
        return out


def solid_wedge(
    x1: float, y1: float, x2: float, y2: float, *, half: float = _WEDGE_HALF
) -> PathPrim:
    """Filled wedge; tip at (x1,y1) stereocenter — RDKit BEGINWEDGE."""
    ux, uy, nx, ny, _ = _unit(x1, y1, x2, y2)
    # Fat end centered on endpoint.
    ax, ay = x2 + nx * half, y2 + ny * half
    bx, by = x2 - nx * half, y2 - ny * half
    return PathPrim(
        d=f"M {x1:.2f} {y1:.2f} L {ax:.2f} {ay:.2f} L {bx:.2f} {by:.2f} Z",
        stroke="#111",
        fill="#111",
        stroke_width=0.5,
        cls="bond bond-wedge-up",
    )


def hashed_wedge(
    x1: float, y1: float, x2: float, y2: float, *, half: float = _WEDGE_HALF, n: int = _HASH_COUNT
) -> list[PathPrim]:
    """Hashed wedge; tip at (x1,y1) — RDKit BEGINDASH (MolDraw2D dashes)."""
    ux, uy, nx, ny, length = _unit(x1, y1, x2, y2)
    # Skip last dash at fat end so incident bonds don't hit a bar (RDKit oneLessDash).
    paths: list[PathPrim] = []
    for i in range(n):
        t = (i + 1) / (n + 1)
        w = half * t
        cx = x1 + ux * length * t
        cy = y1 + uy * length * t
        paths.append(
            _line(
                cx + nx * w,
                cy + ny * w,
                cx - nx * w,
                cy - ny * w,
                cls="bond bond-wedge-down",
                width=1.2,
            )
        )
    return paths


def wavy_bond(x1: float, y1: float, x2: float, y2: float, *, amp: float = 2.0, waves: int = 4) -> PathPrim:
    """Wiggly single for unspecified stereo (RDKit UNKNOWN / Indigo EITHER)."""
    ux, uy, nx, ny, length = _unit(x1, y1, x2, y2)
    parts = [f"M {x1:.2f} {y1:.2f}"]
    steps = waves * 2
    for i in range(1, steps + 1):
        t = i / steps
        side = 1.0 if i % 2 else -1.0
        px = x1 + ux * length * t + nx * amp * side
        py = y1 + uy * length * t + ny * amp * side
        parts.append(f"L {px:.2f} {py:.2f}")
    return PathPrim(
        d=" ".join(parts),
        stroke="#111",
        stroke_width=_STROKE,
        fill="none",
        cls="bond bond-either",
    )


def crossed_double(
    x1: float, y1: float, x2: float, y2: float, interior: tuple[float, float] | None
) -> list[PathPrim]:
    """Crossed double for unspecified E/Z (RDKit EITHERDOUBLE)."""
    ux, uy, lx, ly, length = _unit(x1, y1, x2, y2)
    nx, ny = interior if interior is not None else (lx, ly)
    off = 2.4
    gap = min(3.2, length * 0.16)
    sx1, sy1, sx2, sy2 = shorten(x1, y1, x2, y2, gap, gap)
    # Two diagonals between the parallel offset positions.
    a1x, a1y = sx1 + nx * off, sy1 + ny * off
    a2x, a2y = sx2 + nx * off, sy2 + ny * off
    b1x, b1y = sx1 - nx * off, sy1 - ny * off
    b2x, b2y = sx2 - nx * off, sy2 - ny * off
    return [
        _line(x1, y1, x2, y2, cls="bond bond-skeleton"),
        _line(a1x, a1y, b2x, b2y, cls="bond bond-either-cross"),
        _line(b1x, b1y, a2x, a2y, cls="bond bond-either-cross"),
    ]


def bond_strokes(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    order: float,
    *,
    interior: tuple[float, float] | None = None,
    stereo: str | None = None,
) -> BondStrokes:
    """Build skeleton / offset / stereo strokes for one bond.

    ``stereo``: ``up`` | ``down`` | ``either`` | ``none`` | ``None``.
    Thin end of wedges is at ``(x1,y1)`` — callers must pass stereocenter first
    (BondLayout.begin after backends set begin=stereocenter).
    """
    order = depict_order(order)
    stereo = (stereo or "none").lower()
    if stereo == "none":
        stereo = ""

    # Stereo singles replace the skeleton (RDKit/Indigo).
    if stereo == "up" and order < 1.5:
        return BondStrokes(stereo=[solid_wedge(x1, y1, x2, y2)])
    if stereo == "down" and order < 1.5:
        return BondStrokes(stereo=hashed_wedge(x1, y1, x2, y2))
    if stereo == "either" and order < 1.5:
        return BondStrokes(stereo=[wavy_bond(x1, y1, x2, y2)])
    if stereo == "either" and order >= 1.5:
        return BondStrokes(stereo=crossed_double(x1, y1, x2, y2, interior))

    ux, uy, lx, ly, length = _unit(x1, y1, x2, y2)
    skeleton = _line(x1, y1, x2, y2, cls="bond bond-skeleton")
    if order < 1.5:
        return BondStrokes(skeleton=skeleton)

    nx, ny = interior if interior is not None else (lx, ly)
    offsets: list[PathPrim] = []
    if order >= 2.5:
        gap = min(4.0, length * 0.18)
        sx1, sy1, sx2, sy2 = shorten(x1, y1, x2, y2, gap, gap)
        for side in (-1.0, 1.0):
            ox, oy = nx * 2.6 * side, ny * 2.6 * side
            offsets.append(
                _line(sx1 + ox, sy1 + oy, sx2 + ox, sy2 + oy, cls="bond bond-offset")
            )
    else:
        off = 2.4
        gap = min(3.2, length * 0.16)
        sx1, sy1, sx2, sy2 = shorten(x1, y1, x2, y2, gap, gap)
        offsets.append(
            _line(
                sx1 + nx * off,
                sy1 + ny * off,
                sx2 + nx * off,
                sy2 + ny * off,
                cls="bond bond-offset",
            )
        )
    return BondStrokes(skeleton=skeleton, offsets=offsets)


def bond_paths(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    order: float,
    interior: tuple[float, float] | None = None,
    stereo: str | None = None,
) -> list[PathPrim]:
    """Backward-compatible flat list (skeleton then offsets then stereo)."""
    return bond_strokes(x1, y1, x2, y2, order, interior=interior, stereo=stereo).paint_order()

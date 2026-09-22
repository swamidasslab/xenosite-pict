"""Bond drawing: skeleton centerlines, then multi-bond offsets, then stereo.

1. Every single bond is a **skeleton** centerline (connected graph).
2. Ring doubles keep one line on the ring and a shortened offset inside it.
   Acyclic doubles and triples are **centered** on the bond axis, not shifted
   to one side. A single neighbor is extended to the far line. Two singles
   meet, and the multiple-bond lines stop where they cross those lines.
3. Tetrahedral stereo replaces the skeleton with a solid or hashed wedge.
   Solid (toward the viewer) is thin at the stereocenter and thick at the
   substituent. Hashed (away) is thick at the stereocenter and thin at the
   substituent, so the taper and the hashes say the same thing. ``begin`` is
   always the stereocenter.
4. ``either`` → wavy single or crossed double.
"""

from __future__ import annotations

import math
from collections import defaultdict
from dataclasses import dataclass, field

from xpict.contracts.scene import PathPrim
from xpict.draw.metrics import (
    BOND_PX,
    CHAIN_END_GAP_PX,
    END_GAP_PX,
    OFFSET_PX,
    STROKE_PX,
    WEDGE_HALF_PX,
    hash_count,
)

_STROKE = STROKE_PX
_WEDGE_HALF = WEDGE_HALF_PX


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
        stroke_linecap="round",
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
    _ux, _uy, nx, ny, _ = _unit(x1, y1, x2, y2)
    # Fat end centered on endpoint.
    ax, ay = x2 + nx * half, y2 + ny * half
    bx, by = x2 - nx * half, y2 - ny * half
    return PathPrim(
        d=f"M {x1:.2f} {y1:.2f} L {ax:.2f} {ay:.2f} L {bx:.2f} {by:.2f} Z",
        stroke="#111",
        fill="#111",
        stroke_width=0.6,
        stroke_linecap="round",
        cls="bond bond-wedge-up",
    )


def hashed_wedge(
    x1: float, y1: float, x2: float, y2: float, *, half: float = _WEDGE_HALF, n: int | None = None
) -> list[PathPrim]:
    """Hashed wedge for a bond going away from the viewer.

    ``(x1, y1)`` is the stereocenter. Hashes are wide there and narrow at the
    substituent: the dashes say "away", and a thick end on the stereocenter
    says "away" too. (A thick end on the substituent would say "toward".)
    """
    ux, uy, nx, ny, length = _unit(x1, y1, x2, y2)
    count = n if n is not None else hash_count(length)
    paths: list[PathPrim] = []
    for i in range(count):
        # Inset from both ends so a hash does not sit on the vertex.
        t = (i + 1) / (count + 1)
        w = half * (1.0 - t)
        cx = x1 + ux * length * t
        cy = y1 + uy * length * t
        paths.append(
            _line(
                cx + nx * w,
                cy + ny * w,
                cx - nx * w,
                cy - ny * w,
                cls="bond bond-wedge-down",
                width=_STROKE,
            )
        )
    return paths


def wavy_bond(
    x1: float, y1: float, x2: float, y2: float, *, amp: float | None = None, waves: int = 5
) -> PathPrim:
    """Wiggly single for unspecified stereo (RDKit UNKNOWN / Indigo EITHER)."""
    ux, uy, nx, ny, length = _unit(x1, y1, x2, y2)
    if amp is None:
        amp = 0.055 * BOND_PX
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
        stroke_linecap="round",
        fill="none",
        cls="bond bond-either",
    )


def _offset_gap(length: float, *, chain: bool) -> float:
    """End inset for a parallel offset stroke.

    Ring offsets stay short so they miss adjacent bonds. Acyclic crossed
    doubles keep the chain gap; centered multiples are trimmed by joins.
    """
    px = CHAIN_END_GAP_PX if chain else END_GAP_PX
    return min(px, length * 0.22)


def _multi_bond_offset_py(length: float) -> float:
    """Python fallback (mirrors ``xpict-core::bonds``)."""
    if length < 2.0 * OFFSET_PX:
        return min(OFFSET_PX, length * 0.25)
    return OFFSET_PX


def multi_bond_offset(length: float) -> float:
    """Parallel spacing for double/triple strokes (RDKit ``multipleBondOffset``).

    Uses ``xpict._native`` when built; otherwise Python fallback.
    """
    try:
        from xpict import _native

        return float(_native.multi_bond_offset(length))
    except ImportError:
        return _multi_bond_offset_py(length)


# Parallel lines have |cross(d0, d1)| below this (unit directions).
_JOIN_PARALLEL_EPS = 1e-9
# Reject mitres that run farther than this fraction of the multi-bond length.
_JOIN_MAX_T_FRAC = 0.9


def line_intersect(
    p0x: float,
    p0y: float,
    d0x: float,
    d0y: float,
    p1x: float,
    p1y: float,
    d1x: float,
    d1y: float,
) -> tuple[float, float, float, float] | None:
    """Intersect ``p0 + t d0`` with ``p1 + s d1``.

    Returns ``(t, s, ix, iy)`` or ``None`` when the directions are parallel.
    """
    det = d0x * d1y - d0y * d1x
    if abs(det) < _JOIN_PARALLEL_EPS:
        return None
    dx, dy = p1x - p0x, p1y - p0y
    t = (dx * d1y - dy * d1x) / det
    s = (dx * d0y - dy * d0x) / det
    return t, s, p0x + t * d0x, p0y + t * d0y


def centered_displacements(order: float, off: float) -> list[float]:
    """Signed offsets of a centered multiple bond, along the left normal.

    A double is two lines split evenly about the axis (separation ``off``).
    A triple keeps the axis and one line ``off`` to each side.
    """
    if order >= 2.5:
        return [-off, 0.0, off]
    return [-off * 0.5, off * 0.5]


@dataclass
class DrawnBond:
    """One bond after label inset, before multiple-bond joins."""

    index: int
    begin: int
    end: int
    x1: float
    y1: float
    x2: float
    y2: float
    order: float
    interior: tuple[float, float] | None = None
    stereo: str | None = None
    begin_labeled: bool = False
    end_labeled: bool = False
    # Trim distance at begin, then end, one entry per centered line.
    trims: tuple[list[float], list[float]] | None = None


def _plain_single(bond: DrawnBond) -> bool:
    order = depict_order(bond.order)
    stereo = (bond.stereo or "none").lower()
    return order < 1.5 and stereo in {"", "none"}


def _centered_multi(bond: DrawnBond) -> bool:
    """Acyclic double/triple. Ring bonds stay shifted toward the interior."""
    order = depict_order(bond.order)
    stereo = (bond.stereo or "none").lower()
    if stereo in {"up", "down", "either"}:
        return False
    return bond.interior is None and order >= 1.5


def _end_frame(bond: DrawnBond, at_begin: bool) -> tuple[float, float, float, float, float, float]:
    """``(ex, ey, ux, uy, nx, ny)`` with ``u`` pointing into the bond."""
    ux, uy, nx, ny, _length = _unit(bond.x1, bond.y1, bond.x2, bond.y2)
    if at_begin:
        return bond.x1, bond.y1, ux, uy, nx, ny
    return bond.x2, bond.y2, -ux, -uy, -nx, -ny


def join_centered_multibonds(bonds: list[DrawnBond]) -> None:
    """Mitre acyclic doubles and triples to their single-bond neighbors.

    Each parallel stroke is an infinite line; each neighboring single is another.
    Junction ends are **line–line intersections** (no fixed-length stubs), so
    acute and obtuse angles both close cleanly.

    One single: extend it to the far parallel line; both multiple-bond strokes
    stop on that single.

    Two singles: leave them meeting at the atom; extend each multiple-bond
    stroke past the atom until its end lies on a single (closes the vertex
    angle gap).
    """
    by_atom: dict[int, list[DrawnBond]] = defaultdict(list)
    by_index = {bond.index: bond for bond in bonds}
    for bond in bonds:
        by_atom[bond.begin].append(bond)
        by_atom[bond.end].append(bond)

    # (single index, 0=begin) -> (x, y, distance past the atom)
    moves: dict[tuple[int, int], tuple[float, float, float]] = {}

    for bond in bonds:
        if not _centered_multi(bond):
            continue
        _ux, _uy, _nx, _ny, length = _unit(bond.x1, bond.y1, bond.x2, bond.y2)
        if length < 1.0:
            continue
        off = multi_bond_offset(length)
        disps = centered_displacements(depict_order(bond.order), off)
        # Displacements above are along the begin→end left normal. Flip at end.
        end_disps = (disps, [-d for d in disps])
        trims = [[0.0] * len(disps), [0.0] * len(disps)]
        joined = False
        t_lo, t_hi = -0.5 * length, _JOIN_MAX_T_FRAC * length
        for end_i, atom, labeled in (
            (0, bond.begin, bond.begin_labeled),
            (1, bond.end, bond.end_labeled),
        ):
            if labeled:
                continue
            singles = [
                other
                for other in by_atom[atom]
                if other.index != bond.index and _plain_single(other)
            ]
            if len(singles) == 0 or len(singles) > 2:
                continue
            ex, ey, ux, uy, nx, ny = _end_frame(bond, end_i == 0)
            disps_e = end_disps[end_i]

            # Per multiple-bond line: intersection t along the stroke with each single.
            # Stroke line: (atom + d·n) + t·u. Positive t into the bond; negative past atom.
            line_ts: list[list[float]] = [[] for _ in disps_e]
            for single in singles:
                if single.begin == atom:
                    sx, sy = single.x2, single.y2
                    end_flag = 0
                else:
                    sx, sy = single.x1, single.y1
                    end_flag = 1
                vx, vy = sx - ex, sy - ey
                vlen = math.hypot(vx, vy)
                if vlen < 1e-6:
                    continue
                vhx, vhy = vx / vlen, vy / vlen
                for i, d in enumerate(disps_e):
                    hit = line_intersect(ex + nx * d, ey + ny * d, ux, uy, ex, ey, vhx, vhy)
                    if hit is None:
                        continue
                    ti, _s, _ix, _iy = hit
                    if t_lo < ti < t_hi and abs(ti) > 1e-9:
                        line_ts[i].append(ti)
                if len(singles) == 1:
                    # Grow the single to the far parallel so both strokes meet it.
                    d_far = min(disps_e) if (ux * vhy - uy * vhx) > 0 else max(disps_e)
                    hit = line_intersect(ex + nx * d_far, ey + ny * d_far, ux, uy, ex, ey, vhx, vhy)
                    if hit is not None:
                        t_far, _s, px, py = hit
                        if 0.0 < t_far < t_hi:
                            dist = math.hypot(px - ex, py - ey)
                            key = (single.index, end_flag)
                            prev = moves.get(key)
                            if prev is None or dist > prev[2]:
                                moves[key] = (px, py, dist)

            for i, ts in enumerate(line_ts):
                if not ts:
                    continue
                if len(singles) == 2:
                    # Extend onto the singles (past the atom) so ends lie on them.
                    neg = [t for t in ts if t < 0.0]
                    pos = [t for t in ts if t > 0.0]
                    if neg:
                        trims[end_i][i] = max(neg)  # smallest extension that hits
                    elif pos:
                        trims[end_i][i] = min(pos)
                else:
                    # One single: shorten/extend each stroke onto that single.
                    for ti in ts:
                        if ti > 0.0:
                            cur = trims[end_i][i]
                            if cur <= 0.0 or ti < cur:
                                trims[end_i][i] = ti
                        else:
                            cur = trims[end_i][i]
                            if cur == 0.0 or (cur < 0.0 and ti > cur):
                                trims[end_i][i] = ti
                joined = True
        if joined:
            bond.trims = (trims[0], trims[1])

    for (index, end_flag), (px, py, _dist) in moves.items():
        single = by_index.get(index)
        if single is None:
            continue
        if end_flag == 0:
            single.x1, single.y1 = px, py
        else:
            single.x2, single.y2 = px, py


def crossed_double(
    x1: float, y1: float, x2: float, y2: float, interior: tuple[float, float] | None
) -> list[PathPrim]:
    """Crossed double for unspecified E/Z (RDKit EITHERDOUBLE)."""
    _ux, _uy, lx, ly, length = _unit(x1, y1, x2, y2)
    nx, ny = interior if interior is not None else (lx, ly)
    off = multi_bond_offset(length)
    gap = _offset_gap(length, chain=interior is None)
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
    trims: tuple[list[float], list[float]] | None = None,
) -> BondStrokes:
    """Build skeleton / offset / stereo strokes for one bond.

    ``stereo``: ``up`` | ``down`` | ``either`` | ``none`` | ``None``.
    Thin end of a solid wedge, and the thick end of a hashed wedge, are at
    ``(x1, y1)``. Callers pass the stereocenter first (``BondLayout.begin``).

    ``trims`` are per-line inset distances at ``(x1, y1)`` then ``(x2, y2)``
    for a centered multiple bond (see ``join_centered_multibonds``).
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

    off = multi_bond_offset(length)
    if interior is None:
        return _centered_strokes(x1, y1, x2, y2, order, off, ux, uy, lx, ly, length, trims)

    # Ring double: one line on the cycle, one offset toward the interior.
    # Triples in a ring stay symmetric about the axis, still inset.
    gap = _offset_gap(length, chain=False)
    sx1, sy1, sx2, sy2 = shorten(x1, y1, x2, y2, gap, gap)
    offsets: list[PathPrim] = []
    if order >= 2.5:
        for side in (-1.0, 1.0):
            ox, oy = lx * off * side, ly * off * side
            offsets.append(_line(sx1 + ox, sy1 + oy, sx2 + ox, sy2 + oy, cls="bond bond-offset"))
        return BondStrokes(skeleton=skeleton, offsets=offsets)
    offsets.append(
        _line(
            sx1 + interior[0] * off,
            sy1 + interior[1] * off,
            sx2 + interior[0] * off,
            sy2 + interior[1] * off,
            cls="bond bond-offset",
        )
    )
    return BondStrokes(skeleton=skeleton, offsets=offsets)


def _centered_strokes(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    order: float,
    off: float,
    ux: float,
    uy: float,
    nx: float,
    ny: float,
    length: float,
    trims: tuple[list[float], list[float]] | None,
) -> BondStrokes:
    disps = centered_displacements(order, off)
    t1s = trims[0] if trims is not None else [0.0] * len(disps)
    t2s = trims[1] if trims is not None else [0.0] * len(disps)
    skeleton: PathPrim | None = None
    offsets: list[PathPrim] = []
    for i, disp in enumerate(disps):
        t1 = t1s[i] if i < len(t1s) else 0.0
        t2 = t2s[i] if i < len(t2s) else 0.0
        if t1 + t2 > length * 0.9 and t1 + t2 > 0:
            scale = (length * 0.9) / (t1 + t2)
            t1 *= scale
            t2 *= scale
        ax = x1 + nx * disp + ux * t1
        ay = y1 + ny * disp + uy * t1
        bx = x2 + nx * disp - ux * t2
        by = y2 + ny * disp - uy * t2
        line = _line(
            ax,
            ay,
            bx,
            by,
            cls="bond bond-skeleton" if abs(disp) < 1e-9 else "bond bond-offset",
        )
        if abs(disp) < 1e-9:
            skeleton = line
        else:
            offsets.append(line)
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

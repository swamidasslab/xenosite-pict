"""Coarse collision grid for molecule / title packing.

Not pixel-perfect: ink is stamped onto square cells so clearance checks stay
cheap and the packing logic stays readable.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field


@dataclass
class CollisionGrid:
    """Axis-aligned occupancy stamped at ``cell`` resolution (SVG pixels)."""

    cell: float
    cells: set[tuple[int, int]] = field(default_factory=set)

    def _ix(self, x: float) -> int:
        return int(math.floor(x / self.cell))

    def _iy(self, y: float) -> int:
        return int(math.floor(y / self.cell))

    def mark_box(
        self,
        xmin: float,
        ymin: float,
        xmax: float,
        ymax: float,
        *,
        pad: float = 0.0,
    ) -> None:
        if xmax < xmin:
            xmin, xmax = xmax, xmin
        if ymax < ymin:
            ymin, ymax = ymax, ymin
        xmin -= pad
        ymin -= pad
        xmax += pad
        ymax += pad
        x0, x1 = self._ix(xmin), self._ix(xmax)
        y0, y1 = self._iy(ymin), self._iy(ymax)
        for iy in range(y0, y1 + 1):
            for ix in range(x0, x1 + 1):
                self.cells.add((ix, iy))

    def mark_circle(self, cx: float, cy: float, r: float) -> None:
        self.mark_box(cx - r, cy - r, cx + r, cy + r)

    def mark_segment(
        self,
        x1: float,
        y1: float,
        x2: float,
        y2: float,
        *,
        radius: float,
    ) -> None:
        """Stamp a capsule (segment thickened by ``radius``)."""
        dx, dy = x2 - x1, y2 - y1
        length = math.hypot(dx, dy)
        if length < 1e-9:
            self.mark_circle(x1, y1, radius)
            return
        step = max(self.cell * 0.5, 1.0)
        n = max(1, int(math.ceil(length / step)))
        for i in range(n + 1):
            t = i / n
            self.mark_circle(x1 + dx * t, y1 + dy * t, radius)

    def max_y(self) -> float | None:
        """Bottom edge (SVG +Y down) of occupied cells, or None if empty."""
        if not self.cells:
            return None
        return (max(iy for _ix, iy in self.cells) + 1) * self.cell

    def min_y(self) -> float | None:
        if not self.cells:
            return None
        return min(iy for _ix, iy in self.cells) * self.cell

    def max_x(self) -> float | None:
        if not self.cells:
            return None
        return (max(ix for ix, _iy in self.cells) + 1) * self.cell

    def min_x(self) -> float | None:
        if not self.cells:
            return None
        return min(ix for ix, _iy in self.cells) * self.cell

    def overlaps_box(
        self,
        xmin: float,
        ymin: float,
        xmax: float,
        ymax: float,
        *,
        pad: float = 0.0,
    ) -> bool:
        xmin -= pad
        ymin -= pad
        xmax += pad
        ymax += pad
        x0, x1 = self._ix(xmin), self._ix(xmax)
        y0, y1 = self._iy(ymin), self._iy(ymax)
        for iy in range(y0, y1 + 1):
            for ix in range(x0, x1 + 1):
                if (ix, iy) in self.cells:
                    return True
        return False

    def find_slot(
        self,
        anchor_x: float,
        anchor_y: float,
        width: float,
        height: float,
        *,
        prefer: str = "auto",
        gap: float = 8.0,
        distances: list[float] | None = None,
        pad: float = 0.0,
    ) -> tuple[float, float, str]:
        """Return ``(center_x, center_y, side)`` for a free axis-aligned box.

        Tries the preferred side first (or right/left/top/bottom for
        ``auto``), then other sides, at increasing distances. Falls back to
        the preferred side at the first distance when every slot is taken.
        """
        sides = _side_order(prefer)
        dists = distances or [gap, gap * 1.5, gap * 2.2, gap * 3.2]
        for dist in dists:
            for side in sides:
                cx, cy = _slot_center(anchor_x, anchor_y, width, height, side, dist)
                xmin, ymin = cx - width * 0.5, cy - height * 0.5
                xmax, ymax = cx + width * 0.5, cy + height * 0.5
                if not self.overlaps_box(xmin, ymin, xmax, ymax, pad=pad):
                    return cx, cy, side
        # Crowded — place on the first preferred side anyway.
        side = sides[0]
        cx, cy = _slot_center(anchor_x, anchor_y, width, height, side, dists[0])
        return cx, cy, side


_SIDE_CYCLE = ("right", "left", "top", "bottom")


def _side_order(prefer: str) -> list[str]:
    key = (prefer or "auto").lower()
    if key in ("", "auto"):
        return list(_SIDE_CYCLE)
    if key not in _SIDE_CYCLE:
        return list(_SIDE_CYCLE)
    return [key, *[s for s in _SIDE_CYCLE if s != key]]


def _slot_center(
    ax: float,
    ay: float,
    width: float,
    height: float,
    side: str,
    dist: float,
) -> tuple[float, float]:
    if side == "right":
        return ax + dist + width * 0.5, ay
    if side == "left":
        return ax - dist - width * 0.5, ay
    if side == "top":
        return ax, ay - dist - height * 0.5
    return ax, ay + dist + height * 0.5

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

    def translated(self, dx: float, dy: float) -> CollisionGrid:
        """Return a copy with coordinates shifted by ``(dx, dy)``."""
        if not self.cells or (abs(dx) < 1e-12 and abs(dy) < 1e-12):
            return CollisionGrid(cell=self.cell, cells=set(self.cells))
        sx = int(round(dx / self.cell))
        sy = int(round(dy / self.cell))
        return CollisionGrid(
            cell=self.cell,
            cells={(ix + sx, iy + sy) for ix, iy in self.cells},
        )

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

"""Concentric plot-dot shading (xenopict PlotDot semantics)."""

from __future__ import annotations

from collections.abc import Sequence


class PlotDot:
    """Map z in [-1, 1] to concentric (radius_frac, color_z) rings.

    Rings are sorted weak→strong so intense colors paint on top — same order
    as xenopict.plotdot.PlotDot.
    """

    def __init__(self, levels: int = 4) -> None:
        self.levels = levels
        self.stops = [(i + 1) / levels for i in range(levels)]

    def _radius(self, z: float, level: int) -> float:
        az = abs(z)
        if level == 0:
            return self.stops[0] ** 0.5
        offset = 1.0 - self.stops[level]
        r = az - offset
        return 0.0 if r < self.stops[0] else r**0.5

    def _color(self, z: float, level: int) -> float:
        sign = -1.0 if z < 0 else 1.0
        return z if level == 0 else sign * self.stops[-level - 1]

    def rings(self, z: float) -> list[tuple[float, float]]:
        if abs(z) < 0.05:
            return []
        out = [(self._radius(z, lvl), self._color(z, lvl)) for lvl in range(self.levels)]
        return [(r, c) for r, c in out if r > 0]

    def __call__(
        self, zs: Sequence[float], coords: Sequence[tuple[float, float]]
    ) -> list[tuple[float, float, tuple[float, float]]]:
        dots: list[tuple[float, float, tuple[float, float]]] = []
        for z, coord in zip(zs, coords, strict=False):
            for radius, color in self.rings(float(z)):
                dots.append((radius, color, coord))
        # weak / small first → strong colors on top
        dots.sort(key=lambda d: (abs(d[1]), d[0]))
        return dots

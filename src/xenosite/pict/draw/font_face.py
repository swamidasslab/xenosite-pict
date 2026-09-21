"""Bundled Liberation Sans face — path, TTFont cache, outline pen."""

from __future__ import annotations

from functools import lru_cache
from importlib import resources
from pathlib import Path

from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTFont

_FONT_PACKAGE = "xenosite.pict.data.fonts"
_FONT_NAME = "LiberationSans-Regular.ttf"
_BEZIER_STEPS = 8


def bundled_font_path() -> Path:
    """Filesystem path to the packaged Liberation Sans Regular TTF."""
    root = resources.files(_FONT_PACKAGE)
    return Path(str(root.joinpath(_FONT_NAME)))


@lru_cache(maxsize=1)
def ttfont() -> TTFont:
    return TTFont(bundled_font_path())


@lru_cache(maxsize=1)
def glyph_set_cmap_upem() -> tuple[object, dict[int, str], float]:
    font = ttfont()
    return font.getGlyphSet(), font.getBestCmap() or {}, float(font["head"].unitsPerEm)


class ContourPen(BasePen):
    """Record TrueType contours as polylines (quadratic/cubic sampled)."""

    def __init__(self, glyph_set: object) -> None:
        super().__init__(glyph_set)
        self.contours: list[list[tuple[float, float]]] = []
        self._pts: list[tuple[float, float]] = []

    def _moveTo(self, pt: tuple[float, float]) -> None:
        self._pts = [pt]

    def _lineTo(self, pt: tuple[float, float]) -> None:
        self._pts.append(pt)

    def _curveToOne(
        self,
        p1: tuple[float, float],
        p2: tuple[float, float],
        p3: tuple[float, float],
    ) -> None:
        p0 = self._pts[-1]
        for i in range(1, _BEZIER_STEPS + 1):
            t = i / _BEZIER_STEPS
            mt = 1.0 - t
            x = (
                mt**3 * p0[0]
                + 3 * mt**2 * t * p1[0]
                + 3 * mt * t**2 * p2[0]
                + t**3 * p3[0]
            )
            y = (
                mt**3 * p0[1]
                + 3 * mt**2 * t * p1[1]
                + 3 * mt * t**2 * p2[1]
                + t**3 * p3[1]
            )
            self._pts.append((x, y))

    def _qCurveToOne(self, p1: tuple[float, float], p2: tuple[float, float]) -> None:
        p0 = self._pts[-1]
        for i in range(1, _BEZIER_STEPS + 1):
            t = i / _BEZIER_STEPS
            mt = 1.0 - t
            x = mt**2 * p0[0] + 2 * mt * t * p1[0] + t**2 * p2[0]
            y = mt**2 * p0[1] + 2 * mt * t * p1[1] + t**2 * p2[1]
            self._pts.append((x, y))

    def _closePath(self) -> None:
        if len(self._pts) >= 3:
            self.contours.append(self._pts)
        self._pts = []

    def _endPath(self) -> None:
        self._pts = []

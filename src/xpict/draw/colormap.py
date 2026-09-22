"""Named shade colormaps (xenopict ``cmap`` / ``color_map`` semantics)."""

from __future__ import annotations

from xpict.draw._xenosite_cm import XENOSITE

_COLORMAPS: dict[str, list[list[float]]] = {
    "xenosite": XENOSITE,
}


def sample_colormap(
    z: float,
    *,
    name: str = "xenosite",
    diverging: bool = False,
) -> tuple[float, float, float]:
    """Map a PlotDot color stop to RGB in ``[0, 1]``.

    Matches xenopict ``Xenopict.color_map``: with ``diverging`` False, ``z`` is
    clipped to ``[0, 1]`` and looked up in the named LUT (matplotlib-style).
    With ``diverging`` True, ``z`` in ``[-1, 1]`` is remapped to ``[0, 1]``
    first. Unknown names fall back to ``xenosite``.
    """
    stops = _COLORMAPS.get(name) or XENOSITE
    if diverging:
        t = (float(z) + 1.0) * 0.5
    else:
        t = float(z)
    t = 0.0 if t < 0.0 else 1.0 if t > 1.0 else t
    n = len(stops)
    if n == 1:
        r, g, b = stops[0]
        return float(r), float(g), float(b)
    x = t * (n - 1)
    i = int(x)
    if i >= n - 1:
        r, g, b = stops[-1]
        return float(r), float(g), float(b)
    f = x - i
    a, b_ = stops[i], stops[i + 1]
    return (
        a[0] + (b_[0] - a[0]) * f,
        a[1] + (b_[1] - a[1]) * f,
        a[2] + (b_[2] - a[2]) * f,
    )


def colormap_rgb(
    z: float,
    *,
    name: str = "xenosite",
    diverging: bool = False,
) -> str:
    """CSS ``rgb(r,g,b)`` for a shade stop (xenopict ``_color_to_style``)."""
    r, g, b = sample_colormap(z, name=name, diverging=diverging)
    return f"rgb({int(r * 255 + 0.5)},{int(g * 255 + 0.5)},{int(b * 255 + 0.5)})"

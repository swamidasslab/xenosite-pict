"""Rust core bindings (``xpict._native``). Extension is required on the ship path."""

from __future__ import annotations

import json
from typing import cast

from xpict.contracts.depict import DepictSpec as WireDepictSpec
from xpict.contracts.depict_dict import DepictSpecDict, MolOptsPatchDict
from xpict.contracts.rendered_dict import DocPaintDict, RenderedDict
from xpict.contracts.scene import Scene
from xpict.contracts.scene_dict import SceneDict

try:
    from xpict import _native as _rust
except ImportError as e:  # pragma: no cover
    raise ImportError(
        "xpict requires the Rust extension (xpict._native). "
        "Build with: maturin develop --manifest-path crates/xpict-py/Cargo.toml"
    ) from e

DepictSpecLike = WireDepictSpec | DepictSpecDict | str


def _as_json(obj: DepictSpecLike) -> str:
    if isinstance(obj, str):
        return obj
    dump = getattr(obj, "model_dump", None)
    if callable(dump):
        return json.dumps(dump(mode="json"))
    return json.dumps(obj)


def resolve_depict_chrome(spec: DepictSpecLike) -> list[MolOptsPatchDict]:
    """Resolve cascading mol opts in document order (Rust DepictSpec semantics)."""
    return cast(
        list[MolOptsPatchDict],
        json.loads(_rust.resolve_depict_chrome(_as_json(spec))),
    )


def depict(spec: DepictSpecLike) -> list[RenderedDict]:
    """DepictSpec → ``Rendered`` cells (CoordGen layout + mol-spec paint + pack)."""
    return cast(list[RenderedDict], json.loads(_rust.depict(_as_json(spec))))


def scene_to_svg(scene: Scene | SceneDict | str) -> str:
    """Scene JSON → SVG (Rust core; viewport ids become ``data-id``)."""
    if isinstance(scene, str):
        payload = scene
    else:
        dump = getattr(scene, "model_dump", None)
        if callable(dump):
            payload = json.dumps(dump(mode="json"))
        else:
            payload = json.dumps(scene)
    return _rust.scene_to_svg(payload)


def svg_to_pdf(svg: str) -> bytes:
    """SVG → single-page vector PDF (Rust ``svg2pdf``)."""
    return bytes(_rust.svg_to_pdf(svg))


def svg_to_png(svg: str, scale: float = 2.0) -> bytes:
    """SVG → PNG at ``scale`` × the SVG size (Rust ``resvg``, bundled font)."""
    return bytes(_rust.svg_to_png(svg, scale))


def svg_to_rgba(svg: str, scale: float = 2.0) -> tuple[bytes, int, int]:
    """SVG → raw straight-alpha RGBA8 bytes plus ``(width, height)``."""
    data, w, h = _rust.svg_to_rgba(svg, scale)
    return bytes(data), w, h


def compose_scheme(
    spec: DepictSpecLike, paints: list[DocPaintDict] | str
) -> SceneDict:
    """Pass 3: DepictSpec + DocPaint rows → composed Scene (ELK + overlays)."""
    paints_json = paints if isinstance(paints, str) else json.dumps(paints)
    return cast(SceneDict, json.loads(_rust.compose_scheme(_as_json(spec), paints_json)))

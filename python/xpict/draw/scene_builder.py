"""Build a Scene from layout + PictSpec (xenopict-inspired layers).

Scheme (reaction/network) composition lives in Rust ``compose_scheme`` —
this module only places molecule viewports for row/grid docs.
"""

from __future__ import annotations

from collections.abc import Sequence

from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.scene import Primitive, Scene, Viewport
from xpict.future.spec import LegacyPictSpec, MoleculeSpec
from xpict.draw.drawable import (
    display_text,
    mol_occupancy,
    normalize_coords,
    paint_molecule,
)
from xpict.draw.drawn import Halo
from xpict.draw.markush import apply_rgroup_texts
from xpict.draw.metrics import shared_coord_scale
from xpict.draw.mol_title import pack_label

_GAP = 24.0


def _flat(spec: LegacyPictSpec | object) -> LegacyPictSpec:
    to_legacy = getattr(spec, "to_legacy", None)
    if callable(to_legacy):
        return to_legacy()  # type: ignore[no-any-return]
    return spec  # type: ignore[return-value]


__all__ = [
    "build_scene",
    "grid_positions",
    "molecule_to_viewport",
    "normalize_coords",
    "viewport_size",
]


def _label_text(mol_spec: MoleculeSpec | None) -> str | None:
    if mol_spec is None or mol_spec.label is None:
        return None
    text = mol_spec.label.text.strip()
    return text or None


def viewport_size(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec | None = None,
    *,
    scale: float | None = None,
) -> tuple[float, float]:
    """Viewport width/height including a molecule label when present."""
    coords, width, height = normalize_coords(layout, scale=scale)
    text = _label_text(mol_spec)
    if text is None or mol_spec is None or mol_spec.label is None:
        return width, height
    texts = apply_rgroup_texts(layout, mol_spec, [display_text(a) for a in layout.atoms])
    occ = mol_occupancy(layout, coords, texts)
    pack = pack_label(
        frame_width=width,
        frame_height=height,
        occupancy=occ,
        text=text,
        pos=mol_spec.label.pos,
    )
    return pack.width, pack.height


def molecule_to_viewport(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec,
    *,
    halo: bool = True,
    scale: float | None = None,
) -> Viewport:
    """Paint one molecule via the drawable hierarchy."""
    vp, _halo = paint_molecule(layout, mol_spec, halo=halo, scale=scale)
    return vp


def grid_positions(
    layouts: Sequence[MoleculeLayout],
    columns: int,
    sizes: Sequence[tuple[float, float]],
) -> list[tuple[float, float]]:
    """Top-left positions for a simple grid of viewports."""
    cols = max(1, columns)
    n = len(layouts)
    rows = (n + cols - 1) // cols
    col_w = [0.0] * cols
    row_h = [0.0] * rows
    for i, (w, h) in enumerate(sizes):
        r, c = divmod(i, cols)
        col_w[c] = max(col_w[c], w)
        row_h[r] = max(row_h[r], h)
    xs = [0.0]
    for c in range(cols - 1):
        xs.append(xs[-1] + col_w[c] + _GAP)
    ys = [0.0]
    for r in range(rows - 1):
        ys.append(ys[-1] + row_h[r] + _GAP)
    return [(xs[c], ys[r]) for i in range(n) for r, c in [divmod(i, cols)]]


def build_scene(
    layouts: Sequence[MoleculeLayout],
    mol_specs: Sequence[MoleculeSpec],
    spec: LegacyPictSpec | object,
    positions: Sequence[tuple[float, float]] | None = None,
    *,
    scale: float | None = None,
) -> Scene:
    """Assemble viewports (+ optional document halo). No scheme overlays.

    Reaction / network arrows are composed in Rust ``compose_scheme``.
    """
    spec = _flat(spec)
    if scale is None:
        scale = shared_coord_scale(layouts)
    painted: list[tuple[Viewport, Halo]] = [
        paint_molecule(layout, mol_spec, halo=spec.halo, scale=scale)
        for layout, mol_spec in zip(layouts, mol_specs, strict=True)
    ]
    viewports = [vp for vp, _ in painted]
    if positions is None:
        x = 0.0
        auto: list[tuple[float, float]] = []
        for vp in viewports:
            auto.append((x, 0.0))
            x += vp.width + 16.0
        positions = auto

    placed: list[Viewport] = []
    doc_halo = Halo()
    max_r = max_b = 0.0
    for (vp, mol_halo), (px, py) in zip(painted, positions, strict=True):
        placed.append(vp.model_copy(update={"x": px, "y": py}))
        max_r = max(max_r, px + vp.width)
        max_b = max(max_b, py + vp.height)
        if spec.halo and mol_halo:
            mol_halo.shift(px, py)
            doc_halo.extend(mol_halo.jobs)

    halo_prims: list[Primitive] = []
    if spec.halo and doc_halo:
        prim = doc_halo.to_prim(cls="halo")
        if prim is not None:
            halo_prims = [prim]

    width = max(float(spec.width or 0.0), max_r)
    height = max(float(spec.height or 0.0), max_b)
    return Scene(
        width=width,
        height=height,
        viewports=placed,
        overlays=[],
        halo=halo_prims,
    )

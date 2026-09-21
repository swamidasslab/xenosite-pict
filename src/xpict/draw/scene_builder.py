"""Build a Scene from layout + PictSpec (xenopict-inspired layers)."""

from __future__ import annotations

from collections.abc import Sequence

from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.scene import (
    CirclePrim,
    PathPrim,
    Scene,
    TextPrim,
    Viewport,
)
from xpict.contracts.spec import LegacyPictSpec, MoleculeSpec
from xpict.draw.arrows import diagram_overlays
from xpict.draw.drawable import (
    display_text,
    mol_occupancy,
    normalize_coords,
    paint_molecule,
)
from xpict.draw.drawn import halo_prims
from xpict.draw.glyphs import compile_text_shapes
from xpict.draw.halo import circle_ring_shape, disk_shape
from xpict.draw.markush import apply_rgroup_texts
from xpict.draw.metrics import HALO_STROKE, LABEL_GAP_PX, STROKE_PX
from xpict.draw.mol_title import pack_label
from xpict.draw.paths import ink_from_path_prim


def _flat(spec: LegacyPictSpec | object) -> LegacyPictSpec:
    to_legacy = getattr(spec, "to_legacy", None)
    if callable(to_legacy):
        return to_legacy()  # type: ignore[no-any-return]
    return spec  # type: ignore[return-value]

__all__ = [
    "build_scene",
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
    layout: MoleculeLayout, mol_spec: MoleculeSpec | None = None
) -> tuple[float, float]:
    """Viewport width/height including a molecule label when present."""
    coords, width, height = normalize_coords(layout)
    text = _label_text(mol_spec)
    if text is None or mol_spec is None or mol_spec.label is None:
        return width, height
    texts = apply_rgroup_texts(
        layout, mol_spec, [display_text(a) for a in layout.atoms]
    )
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
) -> Viewport:
    """Paint one molecule via the drawable hierarchy."""
    return paint_molecule(layout, mol_spec, halo=halo)


def build_scene(
    layouts: Sequence[MoleculeLayout],
    mol_specs: Sequence[MoleculeSpec],
    spec: LegacyPictSpec | object,
    positions: Sequence[tuple[float, float]] | None = None,
    edge_paths: Sequence[Sequence[tuple[float, float]] | None] | None = None,
    *,
    diagram_width: float | None = None,
    diagram_height: float | None = None,
) -> Scene:
    spec = _flat(spec)
    viewports = [
        molecule_to_viewport(layout, mol_spec, halo=spec.halo)
        for layout, mol_spec in zip(layouts, mol_specs, strict=True)
    ]
    if positions is None:
        x = 0.0
        auto: list[tuple[float, float]] = []
        for vp in viewports:
            auto.append((x, 0.0))
            x += vp.width + 16.0
        positions = auto

    placed: list[Viewport] = []
    max_r = max_b = 0.0
    for vp, (px, py) in zip(viewports, positions, strict=True):
        placed.append(vp.model_copy(update={"x": px, "y": py}))
        max_r = max(max_r, px + vp.width)
        max_b = max(max_b, py + vp.height)

    if edge_paths:
        for route in edge_paths:
            if not route:
                continue
            for x, y in route:
                max_r = max(max_r, x + 8.0)
                max_b = max(max_b, y + 8.0)

    overlays = diagram_overlays(spec.diagram.edges, placed, edge_paths=edge_paths)
    if spec.halo:
        overlays = _halo_overlay_ink(overlays) + overlays

    width = spec.width or max(max_r, diagram_width or 0.0)
    height = spec.height or max(max_b, diagram_height or 0.0)
    return Scene(width=width, height=height, viewports=placed, overlays=overlays)


def _halo_overlay_ink(prims: Sequence) -> list[PathPrim]:
    """Bottom-most knockouts for document overlays (edge shafts / labels)."""
    out: list[PathPrim] = []
    for prim in prims:
        if isinstance(prim, TextPrim):
            ink = compile_text_shapes(
                prim.text,
                prim.x,
                prim.y,
                font_size=prim.font_size,
                anchor=prim.anchor,
            )
            out.extend(halo_prims(ink, LABEL_GAP_PX, cls="halo label-halo"))
        elif isinstance(prim, PathPrim):
            ink = ink_from_path_prim(prim)
            ink_r = max(prim.stroke_width, STROKE_PX) * 0.5
            dist = max(LABEL_GAP_PX, 0.5 * HALO_STROKE - ink_r)
            out.extend(halo_prims(ink, dist, cls="halo"))
        elif isinstance(prim, CirclePrim):
            if prim.fill not in (None, "none"):
                ink = disk_shape(prim.cx, prim.cy, prim.r)
            else:
                ink = circle_ring_shape(prim.cx, prim.cy, prim.r, prim.stroke_width)
            out.extend(halo_prims(ink, LABEL_GAP_PX, cls="halo"))
    return out

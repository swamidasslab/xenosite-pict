"""Build a Scene from layout + PictSpec (xenopict-inspired layers)."""

from __future__ import annotations

from collections.abc import Sequence

from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.scene import PathPrim, Scene, Viewport
from xpict.contracts.spec import LegacyPictSpec, MoleculeSpec
from xpict.draw.arrows import diagram_overlays
from xpict.draw.drawable import (
    display_text,
    mol_occupancy,
    normalize_coords,
    paint_molecule,
)
from xpict.draw.drawn import Halo
from xpict.draw.markush import apply_rgroup_texts
from xpict.draw.mol_title import pack_label


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
    vp, _halo = paint_molecule(layout, mol_spec, halo=halo)
    return vp


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
    """Assemble viewports + overlays; one document :class:`~xpict.draw.drawn.Halo`.

    Drawables opt into that halo (backbone, element symbols, annotations).
    Molecule captions and diagram arrows do not.
    """
    spec = _flat(spec)
    painted: list[tuple[Viewport, Halo]] = [
        paint_molecule(layout, mol_spec, halo=spec.halo)
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

    if edge_paths:
        for route in edge_paths:
            if not route:
                continue
            for x, y in route:
                max_r = max(max_r, x + 8.0)
                max_b = max(max_b, y + 8.0)

    # Diagram arrows / edge labels are drawn but do not opt into the halo.
    overlays = diagram_overlays(spec.diagram.edges, placed, edge_paths=edge_paths)

    halo_prims: list[PathPrim] = []
    if spec.halo and doc_halo:
        prim = doc_halo.to_prim(cls="halo")
        if prim is not None:
            halo_prims = [prim]

    width = spec.width or max(max_r, diagram_width or 0.0)
    height = spec.height or max(max_b, diagram_height or 0.0)
    return Scene(
        width=width,
        height=height,
        viewports=placed,
        overlays=overlays,
        halo=halo_prims,
    )

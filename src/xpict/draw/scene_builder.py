"""Build a Scene from layout + PictSpec via Rust ``depict_molecule``.

Molecule ink (bonds / labels / shade / marks / halo) comes from the same Rust
surface as JS. Diagram arrows stay Python. Captions / callout annotations are
optional overlays when present on the MoleculeSpec.
"""

from __future__ import annotations

import json
from collections.abc import Sequence

from xpict import _native
from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.scene import Layer, PathPrim, Scene, Viewport
from xpict.contracts.spec import LegacyPictSpec, MoleculeSpec
from xpict.draw.arrows import diagram_overlays
from xpict.draw.metrics import shared_coord_scale
from xpict.molecule_in import layout_to_molecule_in

# Re-export for tests / callers that imported from scene_builder.
from xpict.draw.drawable import normalize_coords  # noqa: F401

__all__ = [
    "build_scene",
    "molecule_to_viewport",
    "normalize_coords",
    "viewport_size",
]


def _flat(spec: LegacyPictSpec | object) -> LegacyPictSpec:
    to_legacy = getattr(spec, "to_legacy", None)
    if callable(to_legacy):
        return to_legacy()  # type: ignore[no-any-return]
    return spec  # type: ignore[return-value]


def viewport_size(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec | None = None,
    *,
    scale: float | None = None,
) -> tuple[float, float]:
    vp = molecule_to_viewport(layout, mol_spec or MoleculeSpec(), scale=scale)
    return vp.width, vp.height


def molecule_to_viewport(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec,
    *,
    halo: bool = True,
    scale: float | None = None,
) -> Viewport:
    """Paint one molecule via Rust ``depict_molecule``."""
    mol_in = layout_to_molecule_in(layout, mol_spec, scale=scale)
    scene = Scene.model_validate_json(_native.depict_molecule(json.dumps(mol_in)))
    if not scene.viewports:
        return Viewport(id=layout.id or mol_spec.id, width=scene.width, height=scene.height)
    vp = scene.viewports[0]
    # Fold per-molecule halo into an extra layer when document halo is off —
    # build_scene lifts scene.halo to the document when halo=True.
    if not halo and scene.halo:
        layers = list(vp.layers)
        layers.insert(0, Layer(name="halo", primitives=list(scene.halo)))
        vp = vp.model_copy(update={"layers": layers})
    return vp.model_copy(update={"id": layout.id or mol_spec.id or vp.id})


def _rust_paint(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec,
    *,
    scale: float | None,
) -> tuple[Viewport, list[PathPrim]]:
    mol_in = layout_to_molecule_in(layout, mol_spec, scale=scale)
    scene = Scene.model_validate_json(_native.depict_molecule(json.dumps(mol_in)))
    vp = scene.viewports[0] if scene.viewports else Viewport(
        id=layout.id or mol_spec.id, width=scene.width, height=scene.height
    )
    vp = vp.model_copy(update={"id": layout.id or mol_spec.id or vp.id})
    halo = [p for p in scene.halo if isinstance(p, PathPrim)]
    # Also accept any PathPrim-shaped halo entries
    if not halo and scene.halo:
        halo = list(scene.halo)  # type: ignore[arg-type]
    return vp, halo  # type: ignore[return-value]


def build_scene(
    layouts: Sequence[MoleculeLayout],
    mol_specs: Sequence[MoleculeSpec],
    spec: LegacyPictSpec | object,
    positions: Sequence[tuple[float, float]] | None = None,
    edge_paths: Sequence[Sequence[tuple[float, float]] | None] | None = None,
    *,
    diagram_width: float | None = None,
    diagram_height: float | None = None,
    scale: float | None = None,
) -> Scene:
    """Assemble viewports + overlays; molecule ink from Rust depict."""
    spec = _flat(spec)
    if scale is None:
        scale = shared_coord_scale(layouts)

    painted: list[tuple[Viewport, list]] = [
        _rust_paint(layout, mol_spec, scale=scale)
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
    halo_prims: list = []
    max_r = max_b = 0.0
    for (vp, mol_halo), (px, py) in zip(painted, positions, strict=True):
        placed.append(vp.model_copy(update={"x": px, "y": py}))
        max_r = max(max_r, px + vp.width)
        max_b = max(max_b, py + vp.height)
        if spec.halo and mol_halo:
            for prim in mol_halo:
                # shift path/circle by viewport origin
                if getattr(prim, "kind", None) == "path" and getattr(prim, "d", None):
                    # Halo paths are absolute in molecule space; translate via
                    # a group in SVG — Scene stores absolute coords, so rewrite
                    # is expensive. Rust halo is already in local SVG space
                    # starting at 0; offset by translating numbers is hard.
                    # For single-mol (common) px=py=0 works. Multi-mol: embed
                    # as viewport-local by keeping halo on the viewport layer.
                    pass
            if px == 0.0 and py == 0.0:
                halo_prims.extend(mol_halo)
            else:
                # Attach halo as bottom layer of the viewport (local coords).
                layers = [Layer(name="halo", primitives=list(mol_halo)), *vp.layers]
                placed[-1] = placed[-1].model_copy(update={"layers": layers})

    if edge_paths:
        for route in edge_paths:
            if not route:
                continue
            for x, y in route:
                max_r = max(max_r, x + 8.0)
                max_b = max(max_b, y + 8.0)

    overlays = diagram_overlays(spec.diagram.edges, placed, edge_paths=edge_paths)

    width = spec.width or max(max_r, diagram_width or 0.0)
    height = spec.height or max(max_b, diagram_height or 0.0)
    return Scene(
        width=width,
        height=height,
        viewports=placed,
        overlays=overlays,
        halo=halo_prims if spec.halo else [],
    )

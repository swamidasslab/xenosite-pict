"""Build a Scene from layout + PictSpec (xenopict-inspired layers)."""

from __future__ import annotations

import math
from collections.abc import Sequence

from xenosite.pict.contracts.layout import MoleculeLayout
from xenosite.pict.contracts.scene import (
    CirclePrim,
    Layer,
    PathPrim,
    Scene,
    TextPrim,
    Viewport,
)
from xenosite.pict.contracts.spec import MoleculeSpec, PictSpec

_LAYER_ORDER = ("shading", "halo", "bonds", "labels", "marks", "overlay")
_SCALE = 28.0
_PAD = 24.0


def normalize_coords(
    layout: MoleculeLayout,
) -> tuple[list[tuple[float, float]], float, float]:
    """Return SVG coords (Y-flipped), width, height."""
    if not layout.atoms:
        return [], _PAD * 2, _PAD * 2
    xs = [a.x for a in layout.atoms]
    ys = [a.y for a in layout.atoms]
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)
    coords: list[tuple[float, float]] = []
    for a in layout.atoms:
        sx = (a.x - min_x) * _SCALE + _PAD
        sy = (max_y - a.y) * _SCALE + _PAD
        coords.append((sx, sy))
    width = (max_x - min_x) * _SCALE + 2 * _PAD
    height = (max_y - min_y) * _SCALE + 2 * _PAD
    return coords, max(width, 2 * _PAD), max(height, 2 * _PAD)


def _bond_paths(x1: float, y1: float, x2: float, y2: float, order: float) -> list[PathPrim]:
    dx, dy = x2 - x1, y2 - y1
    length = math.hypot(dx, dy) or 1.0
    nx, ny = -dy / length, dx / length
    offset = 3.0
    if order >= 2.5:
        offsets = (-offset, 0.0, offset)
    elif order >= 1.5:
        offsets = (-offset * 0.6, offset * 0.6)
    else:
        offsets = (0.0,)
    out: list[PathPrim] = []
    for o in offsets:
        ox, oy = nx * o, ny * o
        out.append(
            PathPrim(
                d=f"M {x1 + ox:.2f} {y1 + oy:.2f} L {x2 + ox:.2f} {y2 + oy:.2f}",
                stroke="#111",
                stroke_width=1.6,
                cls="bond",
            )
        )
    return out


def _shade_color(z: float, vmin: float, vmax: float) -> str:
    if vmax <= vmin:
        t = 0.5
    else:
        t = max(0.0, min(1.0, (z - vmin) / (vmax - vmin)))
    r = int(255 - t * (255 - 70))
    g = int(255 - t * (255 - 130))
    b = int(255 - t * (255 - 180))
    return f"rgb({r},{g},{b})"


def molecule_to_viewport(layout: MoleculeLayout, mol_spec: MoleculeSpec) -> Viewport:
    coords, width, height = normalize_coords(layout)
    layers: dict[str, Layer] = {name: Layer(name=name) for name in _LAYER_ORDER}  # type: ignore[arg-type]

    if mol_spec.shade and mol_spec.shade.atoms and coords:
        zs = mol_spec.shade.atoms
        vmin = mol_spec.shade.vmin if mol_spec.shade.vmin is not None else min(zs)
        vmax = mol_spec.shade.vmax if mol_spec.shade.vmax is not None else max(zs)
        for i, z in enumerate(zs):
            if i >= len(coords):
                break
            x, y = coords[i]
            layers["shading"].primitives.append(
                CirclePrim(
                    cx=x,
                    cy=y,
                    r=_SCALE * 0.45,
                    fill=_shade_color(z, vmin, vmax),
                    opacity=0.85,
                    cls=f"atom-{i} shade",
                )
            )

    atom_pos = {a.index: i for i, a in enumerate(layout.atoms)}
    bond_color = mol_spec.color or "#111"
    for bond in layout.bonds:
        i0, i1 = atom_pos.get(bond.begin), atom_pos.get(bond.end)
        if i0 is None or i1 is None:
            continue
        x1, y1 = coords[i0]
        x2, y2 = coords[i1]
        for p in _bond_paths(x1, y1, x2, y2, bond.order):
            p.stroke = bond_color
            p.cls = f"bond-{bond.index} atom-{bond.begin} atom-{bond.end}"
            layers["bonds"].primitives.append(p)

    if mol_spec.halo:
        for prim in list(layers["bonds"].primitives):
            if isinstance(prim, PathPrim):
                layers["halo"].primitives.append(
                    PathPrim(
                        d=prim.d,
                        stroke="#fff",
                        stroke_width=prim.stroke_width + 4.0,
                        opacity=0.85,
                        cls="halo",
                    )
                )

    for i, atom in enumerate(layout.atoms):
        label = atom.label
        if label is None and atom.charge:
            label = atom.element
        if not label:
            continue
        x, y = coords[i]
        text = label
        if atom.charge:
            sign = "+" if atom.charge > 0 else "-"
            mag = abs(atom.charge)
            text = f"{label}{sign}" if mag == 1 else f"{label}{mag}{sign}"
        if mol_spec.halo:
            layers["halo"].primitives.append(
                CirclePrim(cx=x, cy=y, r=9.0, fill="#fff", opacity=0.9, cls="label-halo")
            )
        layers["labels"].primitives.append(
            TextPrim(x=x, y=y + 4, text=text, cls=f"atom-{atom.index} label")
        )

    for mark in mol_spec.marks:
        color = mark.color or "#c44"
        if mark.atoms:
            for ai in mark.atoms:
                pos = atom_pos.get(ai)
                if pos is None:
                    continue
                x, y = coords[pos]
                layers["marks"].primitives.append(
                    CirclePrim(
                        cx=x,
                        cy=y,
                        r=_SCALE * 0.35,
                        fill="none",
                        stroke=color,
                        stroke_width=2.2,
                        opacity=0.75,
                        cls=f"atom-{ai} mark",
                    )
                )
        if mark.bonds:
            for a, b in mark.bonds:
                ia, ib = atom_pos.get(a), atom_pos.get(b)
                if ia is None or ib is None:
                    continue
                x1, y1 = coords[ia]
                x2, y2 = coords[ib]
                layers["marks"].primitives.append(
                    PathPrim(
                        d=f"M {x1:.2f} {y1:.2f} L {x2:.2f} {y2:.2f}",
                        stroke=color,
                        stroke_width=6.0,
                        opacity=0.35,
                        cls=f"bond-mark atom-{a} atom-{b}",
                    )
                )

    return Viewport(
        id=layout.id or mol_spec.id,
        width=width,
        height=height,
        layers=[layers[n] for n in _LAYER_ORDER],  # type: ignore[index]
    )


def build_scene(
    layouts: Sequence[MoleculeLayout],
    mol_specs: Sequence[MoleculeSpec],
    spec: PictSpec,
    positions: Sequence[tuple[float, float]] | None = None,
) -> Scene:
    viewports = [
        molecule_to_viewport(layout, mol_spec)
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

    return Scene(
        width=spec.width or max_r,
        height=spec.height or max_b,
        viewports=placed,
    )

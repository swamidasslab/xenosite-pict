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
from xenosite.pict.contracts.spec import MarkKind, MoleculeSpec, PictSpec
from xenosite.pict.draw.arrows import diagram_overlays
from xenosite.pict.draw.bonds import (
    DrawnBond,
    bond_paths,
    bond_strokes,
    depict_order,
    join_centered_multibonds,
    shorten,
)
from xenosite.pict.draw.metrics import (
    BOND_PX,
    FONT_PX,
    HALO_STROKE,
    MARK_FRAC,
    PAD_PX,
    RADICAL_BASE,
    RADICAL_BASE_BARE,
    RADICAL_DOT_R,
    SHADE_FRAC,
    STROKE_PX,
    coord_scale,
    label_clearance,
)
from xenosite.pict.draw.plotdot import PlotDot
from xenosite.pict.draw.rings import bond_interior_normals, find_sssr

_LAYER_ORDER = ("shading", "halo", "bonds", "labels", "marks", "overlay")
_PAD = PAD_PX


def normalize_coords(
    layout: MoleculeLayout,
) -> tuple[list[tuple[float, float]], float, float]:
    """Return SVG coords (Y-flipped), width, height.

    Scale so the mean layout bond draws at ``BOND_PX`` (Indigo bonds are ~1,
    native bonds are 1.5 — both should depict the same size).
    """
    if not layout.atoms:
        return [], _PAD * 2, _PAD * 2
    scale = coord_scale(layout)
    xs = [a.x for a in layout.atoms]
    ys = [a.y for a in layout.atoms]
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)
    coords = [
        ((a.x - min_x) * scale + _PAD, (max_y - a.y) * scale + _PAD) for a in layout.atoms
    ]
    width = (max_x - min_x) * scale + 2 * _PAD
    height = (max_y - min_y) * scale + 2 * _PAD
    return coords, max(width, 2 * _PAD), max(height, 2 * _PAD)


def _display_text(atom) -> str | None:
    """Label string actually painted (includes charge suffix)."""
    label = atom.label
    if label is None and (atom.charge or atom.radical or atom.element == "*"):
        label = "*" if atom.element == "*" else atom.element
    if not label:
        return None
    text = label
    if atom.charge:
        sign = "+" if atom.charge > 0 else "−"  # unicode minus for depiction
        mag = abs(atom.charge)
        # Charge as a tight suffix (NH4+, O−); H count already lives in the label.
        text = f"{label}{sign}" if mag == 1 else f"{label}{mag}{sign}"
    return text


def _shorten(
    x1: float, y1: float, x2: float, y2: float, gap1: float, gap2: float
) -> tuple[float, float, float, float]:
    return shorten(x1, y1, x2, y2, gap1, gap2)


def _depict_order(order: float) -> float:
    return depict_order(order)


def _bond_key(a: int, b: int) -> tuple[int, int]:
    return (a, b) if a < b else (b, a)


def _ring_bond_normals(
    layout: MoleculeLayout, coords: list[tuple[float, float]]
) -> dict[tuple[int, int], tuple[float, float]]:
    """Unit normals for ring bonds via SSSR centroids (see ``draw.rings``).

    Layout backends (CDK RingPlacer / RDKit embedRing / Indigo layout) own
    regular-polygon coordinates. The drawer only needs SSSR membership to put
    Kekulé offsets toward ring interiors.
    """
    rings = find_sssr(layout)
    if not rings:
        return {}
    coords_by_index = {a.index: coords[i] for i, a in enumerate(layout.atoms)}
    return bond_interior_normals(rings, coords_by_index)


def _bond_paths(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    order: float,
    interior: tuple[float, float] | None = None,
    stereo: str | None = None,
) -> list[PathPrim]:
    """Skeleton centerline, then offsets, then stereo (see ``draw.bonds``)."""
    return bond_paths(x1, y1, x2, y2, order, interior=interior, stereo=stereo)


def _normalize_shade_scores(zs: list[float], vmin: float, vmax: float) -> list[float]:
    """Map shade values into roughly [-1, 1] without forcing zeros to ±1.

    All-nonnegative scores (common for site-of-metabolism style) stay on the
    positive half so 0 remains white. True diverging data keeps 0 fixed.
    """
    if vmin >= 0 and vmax > 0:
        return [max(0.0, min(1.0, z / vmax)) for z in zs]
    if vmax <= 0 and vmin < 0:
        return [max(-1.0, min(0.0, z / abs(vmin))) for z in zs]
    scale = max(abs(vmin), abs(vmax), 1e-9)
    return [max(-1.0, min(1.0, z / scale)) for z in zs]


def _shade_rgb(z: float) -> str:
    """Diverging blue←white→red; soft near zero so mid scores stay pale."""
    z = max(-1.0, min(1.0, z))
    # Square keeps |z|<~0.4 nearly white (xenopict-ish soft ramp).
    t = abs(z) ** 1.35
    if z >= 0:
        r, g, b = 255, int(255 - t * 210), int(255 - t * 210)
    else:
        r, g, b = int(255 - t * 210), int(255 - t * 170), 255
    return f"rgb({r},{g},{b})"

def _convex_hull(points: list[tuple[float, float]]) -> list[tuple[float, float]]:
    pts = sorted(set(points))
    if len(pts) <= 2:
        return pts

    def cross(o: tuple[float, float], a: tuple[float, float], b: tuple[float, float]) -> float:
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower: list[tuple[float, float]] = []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    upper: list[tuple[float, float]] = []
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return lower[:-1] + upper[:-1]


def _hull_path(points: list[tuple[float, float]], pad: float = 10.0) -> str | None:
    if not points:
        return None
    if len(points) == 1:
        x, y = points[0]
        r = pad
        return (
            f"M {x + r:.2f} {y:.2f} "
            f"A {r:.2f} {r:.2f} 0 1 0 {x - r:.2f} {y:.2f} "
            f"A {r:.2f} {r:.2f} 0 1 0 {x + r:.2f} {y:.2f}"
        )
    hull = _convex_hull(points)
    if len(hull) == 2:
        (x1, y1), (x2, y2) = hull
        dx, dy = x2 - x1, y2 - y1
        length = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / length * pad, dx / length * pad
        return (
            f"M {x1 + nx:.2f} {y1 + ny:.2f} L {x2 + nx:.2f} {y2 + ny:.2f} "
            f"L {x2 - nx:.2f} {y2 - ny:.2f} L {x1 - nx:.2f} {y1 - ny:.2f} Z"
        )
    cx = sum(p[0] for p in hull) / len(hull)
    cy = sum(p[1] for p in hull) / len(hull)
    expanded = []
    for x, y in hull:
        dx, dy = x - cx, y - cy
        length = math.hypot(dx, dy) or 1.0
        expanded.append((x + dx / length * pad, y + dy / length * pad))
    d = f"M {expanded[0][0]:.2f} {expanded[0][1]:.2f} "
    d += " ".join(f"L {x:.2f} {y:.2f}" for x, y in expanded[1:])
    return d + " Z"


def molecule_to_viewport(layout: MoleculeLayout, mol_spec: MoleculeSpec) -> Viewport:
    coords, width, height = normalize_coords(layout)
    layers: dict[str, Layer] = {name: Layer(name=name) for name in _LAYER_ORDER}  # type: ignore[arg-type]
    atom_pos = {a.index: i for i, a in enumerate(layout.atoms)}
    ring_normals = _ring_bond_normals(layout, coords)

    if mol_spec.shade and mol_spec.shade.atoms and coords:
        zs = list(mol_spec.shade.atoms)
        vmin = mol_spec.shade.vmin if mol_spec.shade.vmin is not None else min(zs)
        vmax = mol_spec.shade.vmax if mol_spec.shade.vmax is not None else max(zs)
        norm = _normalize_shade_scores(zs, vmin, vmax)
        # xenopict shade() scales plot-dots by scale * 0.9
        base_r = BOND_PX * SHADE_FRAC
        for radius_frac, color_z, (x, y) in PlotDot()(norm, coords[: len(norm)]):
            if abs(color_z) < 0.05 and radius_frac < 0.35:
                continue
            if abs(color_z) < 0.02:
                continue
            layers["shading"].primitives.append(
                CirclePrim(
                    cx=x,
                    cy=y,
                    r=base_r * radius_frac,
                    fill=_shade_rgb(color_z),
                    opacity=1.0,
                    cls="shade",
                )
            )

    texts = [_display_text(a) for a in layout.atoms]
    bond_color = mol_spec.color or "#111"
    # Singles first, then centered or interior offsets, then stereo.
    prepared: list[DrawnBond] = []
    for bond in layout.bonds:
        i0, i1 = atom_pos.get(bond.begin), atom_pos.get(bond.end)
        if i0 is None or i1 is None:
            continue
        x1, y1 = coords[i0]
        x2, y2 = coords[i1]
        g1 = label_clearance(texts[i0]) if texts[i0] else 0.0
        g2 = label_clearance(texts[i1]) if texts[i1] else 0.0
        x1, y1, x2, y2 = _shorten(x1, y1, x2, y2, g1, g2)
        interior = ring_normals.get(_bond_key(bond.begin, bond.end))
        prepared.append(
            DrawnBond(
                index=bond.index,
                begin=bond.begin,
                end=bond.end,
                x1=x1,
                y1=y1,
                x2=x2,
                y2=y2,
                order=bond.order,
                interior=interior,
                stereo=bond.stereo,
                begin_labeled=bool(texts[i0]),
                end_labeled=bool(texts[i1]),
            )
        )
    join_centered_multibonds(prepared)
    skeletons: list[PathPrim] = []
    offsets: list[PathPrim] = []
    stereos: list[PathPrim] = []
    for bond in prepared:
        strokes = bond_strokes(
            bond.x1,
            bond.y1,
            bond.x2,
            bond.y2,
            bond.order,
            interior=bond.interior,
            stereo=bond.stereo,
            trims=bond.trims,
        )
        tag = f"bond-{bond.index} atom-{bond.begin} atom-{bond.end}"
        if strokes.skeleton is not None:
            strokes.skeleton.stroke = bond_color
            strokes.skeleton.cls = f"{tag} bond-skeleton"
            skeletons.append(strokes.skeleton)
        for p in strokes.offsets:
            p.stroke = bond_color
            p.cls = f"{tag} bond-offset"
            offsets.append(p)
        for p in strokes.stereo:
            p.stroke = bond_color
            if p.fill and p.fill not in ("none", None):
                p.fill = bond_color
            p.cls = f"{tag} {p.cls or 'bond-stereo'}"
            stereos.append(p)
    painted = [*skeletons, *offsets, *stereos]
    # Halo reuses every bond stroke in white at 2× width, round caps, under
    # the ink. One halo per stroke so double-bond offsets are knocked out
    # too, not only the centerline.
    if mol_spec.halo:
        for p in painted:
            layers["halo"].primitives.append(
                p.model_copy(
                    update={
                        "stroke": "#fff",
                        "fill": "#fff" if p.fill not in (None, "none") else "none",
                        "stroke_width": max(HALO_STROKE, p.stroke_width * 2),
                        "opacity": 1.0,
                        "cls": "halo",
                        "stroke_linecap": "round",
                    }
                )
            )
    layers["bonds"].primitives.extend(painted)

    for i, atom in enumerate(layout.atoms):
        label = texts[i]
        x, y = coords[i]
        # Radical dots (RDKit/Indigo-style): sit beside the atom, outside the label.
        if atom.radical > 0:
            # Prefer a free angular wedge away from bonds.
            nbr_angs = []
            for bond in layout.bonds:
                if bond.begin == atom.index or bond.end == atom.index:
                    other = bond.end if bond.begin == atom.index else bond.begin
                    oi = atom_pos.get(other)
                    if oi is not None:
                        nbr_angs.append(math.atan2(coords[oi][1] - y, coords[oi][0] - x))
            if nbr_angs:
                nbr_angs.sort()
                # Largest gap midpoint.
                best_mid, best_w = nbr_angs[0] + math.pi, 0.0
                for j, a0 in enumerate(nbr_angs):
                    a1 = nbr_angs[(j + 1) % len(nbr_angs)]
                    w = a1 - a0
                    if w <= 0:
                        w += 2 * math.pi
                    if w > best_w:
                        best_w = w
                        best_mid = a0 + w / 2
                ang = best_mid
            else:
                ang = -math.pi / 2
            dot_r = RADICAL_DOT_R
            base = RADICAL_BASE if label else RADICAL_BASE_BARE
            for k in range(min(atom.radical, 3)):
                spread = (k - (min(atom.radical, 3) - 1) / 2) * 0.35
                dang = ang + spread
                layers["labels"].primitives.append(
                    CirclePrim(
                        cx=x + math.cos(dang) * base,
                        cy=y + math.sin(dang) * base,
                        r=dot_r,
                        fill="#111",
                        stroke="none",
                        opacity=1.0,
                        cls=f"atom-{atom.index} radical",
                    )
                )
        if not label:
            continue
        if mol_spec.halo:
            layers["halo"].primitives.append(
                CirclePrim(
                    cx=x,
                    cy=y,
                    r=max(FONT_PX * 0.62, label_clearance(label)),
                    fill="#fff",
                    opacity=1.0,
                    cls="label-halo",
                )
            )
        layers["labels"].primitives.append(
            TextPrim(
                x=x,
                y=y + FONT_PX * 0.35,
                text=label,
                font_size=FONT_PX,
                cls=f"atom-{atom.index} label",
            )
        )

    for mark in mol_spec.marks:
        color = mark.color or "#c44"
        if mark.kind == MarkKind.substructure and mark.atoms:
            pts = [coords[atom_pos[a]] for a in mark.atoms if a in atom_pos]
            path = _hull_path(pts, pad=BOND_PX * 0.45)
            if path:
                layers["marks"].primitives.append(
                    PathPrim(
                        d=path,
                        stroke=color,
                        fill=color,
                        stroke_width=1.5,
                        opacity=0.25,
                        cls="substructure-mark",
                    )
                )
            continue
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
                        r=BOND_PX * MARK_FRAC,
                        fill="none",
                        stroke=color,
                        stroke_width=STROKE_PX,
                        opacity=0.85,
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
                        stroke_width=HALO_STROKE,
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
    edge_paths: Sequence[Sequence[tuple[float, float]] | None] | None = None,
    *,
    diagram_width: float | None = None,
    diagram_height: float | None = None,
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

    # Edge routes may extend slightly past node boxes (ELK root bounds).
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
    )

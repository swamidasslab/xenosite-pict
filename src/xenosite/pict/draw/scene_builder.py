"""Build a Scene from layout + PictSpec (xenopict-inspired layers)."""

from __future__ import annotations

import math
from collections.abc import Sequence
from dataclasses import replace

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
from xenosite.pict.draw.annotate import draw_annotations
from xenosite.pict.draw.arrows import diagram_overlays
from xenosite.pict.draw.bonds import (
    DrawnBond,
    bond_paths,
    bond_strokes,
    depict_order,
    join_centered_multibonds,
    shorten,
)
from xenosite.pict.draw.collision import CollisionGrid
from xenosite.pict.draw.glyphs import compile_text_shapes
from xenosite.pict.draw.halo import (
    capsule_shape,
    circle_ring_shape,
    disk_shape,
    halo_path_d,
    path_polyline_shape,
)
from xenosite.pict.draw.metrics import (
    BOND_PX,
    COLLISION_CELL_PX,
    FONT_PX,
    HALO_STROKE,
    LABEL_GAP_PX,
    MARK_FRAC,
    OFFSET_PX,
    PAD_PX,
    RADICAL_BASE,
    RADICAL_BASE_BARE,
    RADICAL_DOT_R,
    SHADE_FRAC,
    STROKE_PX,
    coord_scale,
    label_clearance,
)
from xenosite.pict.draw.mol_title import pack_label
from xenosite.pict.draw.plotdot import PlotDot
from xenosite.pict.draw.rings import bond_interior_normals, find_sssr
from xenosite.pict.draw.text_metrics import label_baseline_offset, text_box

_LAYER_ORDER = ("halo", "shading", "bonds", "labels", "marks", "overlay")
_PAD = PAD_PX


def _push_halo(
    layers: dict[str, Layer],
    ink,
    *,
    dist: float | None = None,
    cls: str = "halo",
) -> None:
    """Append a white knockout derived from ``ink`` via :func:`halo_from_shapes`.

    Halos are emitted only into the bottom ``halo`` layer so they never
    obscure bonds, labels, marks, or overlays.
    """
    d = halo_path_d(ink, dist)
    if not d:
        return
    layers["halo"].primitives.append(
        PathPrim(
            d=d,
            stroke="none",
            fill="#fff",
            stroke_width=0.0,
            opacity=1.0,
            cls=cls,
        )
    )


def _path_coords(d: str) -> list[tuple[float, float]]:
    """Best-effort point list from an SVG path ``d`` (M/L/Z)."""
    import re

    nums = [float(x) for x in re.findall(r"[-+]?(?:\d+\.?\d*|\.\d+)", d)]
    pts: list[tuple[float, float]] = []
    for i in range(0, len(nums) - 1, 2):
        pts.append((nums[i], nums[i + 1]))
    return pts


def _ink_from_stroke_path(p: PathPrim):
    """Approximate a stroked/filled PathPrim as ink geometry for haloing."""
    from shapely.geometry import Polygon

    pts = _path_coords(p.d)
    if not pts:
        return None
    if p.fill not in (None, "none") and len(pts) >= 3:
        poly = Polygon(pts)
        if not poly.is_valid:
            poly = poly.buffer(0)
        return None if poly.is_empty else poly
    radius = max(p.stroke_width, STROKE_PX) * 0.5
    return path_polyline_shape(pts, radius)


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


def _mol_occupancy(
    layout: MoleculeLayout,
    coords: list[tuple[float, float]],
    texts: list[str | None],
) -> CollisionGrid:
    """Coarse ink occupancy for title packing (bonds + atom labels)."""
    grid = CollisionGrid(cell=COLLISION_CELL_PX)
    atom_pos = {a.index: i for i, a in enumerate(layout.atoms)}
    radius = STROKE_PX + 0.5 * OFFSET_PX
    for bond in layout.bonds:
        i0, i1 = atom_pos.get(bond.begin), atom_pos.get(bond.end)
        if i0 is None or i1 is None:
            continue
        x1, y1 = coords[i0]
        x2, y2 = coords[i1]
        grid.mark_segment(x1, y1, x2, y2, radius=radius)
    for i, text in enumerate(texts):
        x, y = coords[i]
        if text:
            ly = y + label_baseline_offset(FONT_PX)
            box = text_box(text, x, ly, font_size=FONT_PX, which="ink", pad=LABEL_GAP_PX * 0.25)
            grid.mark_box(*box.as_tuple())
        else:
            grid.mark_circle(x, y, STROKE_PX)
    return grid


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
    texts = [_display_text(a) for a in layout.atoms]
    occ = _mol_occupancy(layout, coords, texts)
    pack = pack_label(
        frame_width=width,
        frame_height=height,
        occupancy=occ,
        text=text,
        pos=mol_spec.label.pos,
    )
    return pack.width, pack.height


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


def _shift_path_d(d: str, dx: float, dy: float) -> str:
    """Translate M/L coordinates in a simple path ``d`` (A radii untouched)."""
    import re

    # Match number pairs after M/L; leave A radii/flags alone by only
    # shifting coordinates that appear as sequential float pairs in M/L spans.
    # For our paths (M/L/Z and elliptical arcs from oval), shift every
    # coordinate pair that is an absolute point. Arc endpoints are the last
    # pair of each A command — handle via token walk.
    tokens = re.findall(r"[MLZA]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?", d)
    out: list[str] = []
    i = 0
    cmd = "M"
    while i < len(tokens):
        t = tokens[i]
        if t in "MLZA":
            cmd = t
            out.append(t)
            i += 1
            continue
        if cmd in ("M", "L"):
            x = float(t)
            y = float(tokens[i + 1])
            out.append(f"{x + dx:.2f}")
            out.append(f"{y + dy:.2f}")
            i += 2
            continue
        if cmd == "A":
            # A rx ry xrot large sweep x y  — shift only the endpoint.
            nums = [float(tokens[j]) for j in range(i, i + 7)]
            nums[5] += dx
            nums[6] += dy
            out.extend(f"{n:.2f}" if j >= 5 else f"{n:g}" for j, n in enumerate(nums))
            i += 7
            continue
        if cmd == "Z":
            i += 1
            continue
        out.append(t)
        i += 1
    return " ".join(out)


def _shift_layers(layers: dict[str, Layer], dx: float, dy: float) -> None:
    """Translate every primitive in ``layers`` by ``(dx, dy)``."""
    if abs(dx) < 1e-12 and abs(dy) < 1e-12:
        return
    for layer in layers.values():
        shifted: list = []
        for p in layer.primitives:
            if isinstance(p, PathPrim):
                shifted.append(p.model_copy(update={"d": _shift_path_d(p.d, dx, dy)}))
            elif isinstance(p, CirclePrim):
                shifted.append(p.model_copy(update={"cx": p.cx + dx, "cy": p.cy + dy}))
            elif isinstance(p, TextPrim):
                shifted.append(p.model_copy(update={"x": p.x + dx, "y": p.y + dy}))
            else:
                shifted.append(p)
        layer.primitives = shifted


def molecule_to_viewport(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec,
    *,
    halo: bool = True,
) -> Viewport:
    coords, width, height = normalize_coords(layout)
    layers: dict[str, Layer] = {name: Layer(name=name) for name in _LAYER_ORDER}  # type: ignore[arg-type]
    texts = [_display_text(a) for a in layout.atoms]
    label_pack = None
    label_text = _label_text(mol_spec)
    if label_text is not None and mol_spec.label is not None:
        occ = _mol_occupancy(layout, coords, texts)
        label_pack = pack_label(
            frame_width=width,
            frame_height=height,
            occupancy=occ,
            text=label_text,
            pos=mol_spec.label.pos,
        )
        if label_pack.dx or label_pack.dy:
            coords = [
                (x + label_pack.dx, y + label_pack.dy) for x, y in coords
            ]
        width, height = label_pack.width, label_pack.height

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
    layers["bonds"].primitives.extend(painted)
    # Halos from ink shapes — bottom layer only; never covers drawn content.
    if halo:
        for p in painted:
            ink = _ink_from_stroke_path(p)
            if ink is None:
                continue
            # Grow from stroke radius out to ~HALO_STROKE/2 overall.
            ink_r = max(p.stroke_width, STROKE_PX) * 0.5
            dist = max(LABEL_GAP_PX, 0.5 * HALO_STROKE - ink_r)
            _push_halo(layers, ink, dist=dist, cls="halo")

    for i, atom in enumerate(layout.atoms):
        label = texts[i]
        x, y = coords[i]
        # Radical dots (RDKit/Indigo-style): sit beside the atom, outside the label.
        if atom.radical > 0:
            nbr_angs = []
            for bond in layout.bonds:
                if bond.begin == atom.index or bond.end == atom.index:
                    other = bond.end if bond.begin == atom.index else bond.begin
                    oi = atom_pos.get(other)
                    if oi is not None:
                        nbr_angs.append(math.atan2(coords[oi][1] - y, coords[oi][0] - x))
            if nbr_angs:
                nbr_angs.sort()
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
                cx = x + math.cos(dang) * base
                cy = y + math.sin(dang) * base
                layers["labels"].primitives.append(
                    CirclePrim(
                        cx=cx,
                        cy=cy,
                        r=dot_r,
                        fill="#111",
                        stroke="none",
                        opacity=1.0,
                        cls=f"atom-{atom.index} radical",
                    )
                )
                if halo:
                    _push_halo(
                        layers,
                        disk_shape(cx, cy, dot_r),
                        dist=LABEL_GAP_PX,
                        cls="halo radical-halo",
                    )
        if not label:
            continue
        label_y = y + label_baseline_offset(FONT_PX)
        layers["labels"].primitives.append(
            TextPrim(
                x=x,
                y=label_y,
                text=label,
                font_size=FONT_PX,
                cls=f"atom-{atom.index} label",
            )
        )
        if halo:
            ink = compile_text_shapes(label, x, label_y, font_size=FONT_PX)
            _push_halo(layers, ink, dist=LABEL_GAP_PX, cls="halo label-halo")

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
                if halo:
                    ink = _ink_from_stroke_path(
                        PathPrim(d=path, fill=color, stroke=color, stroke_width=1.5)
                    )
                    _push_halo(layers, ink, dist=LABEL_GAP_PX, cls="halo mark-halo")
            continue
        if mark.atoms:
            for ai in mark.atoms:
                pos = atom_pos.get(ai)
                if pos is None:
                    continue
                x, y = coords[pos]
                r = BOND_PX * MARK_FRAC
                layers["marks"].primitives.append(
                    CirclePrim(
                        cx=x,
                        cy=y,
                        r=r,
                        fill="none",
                        stroke=color,
                        stroke_width=STROKE_PX,
                        opacity=0.85,
                        cls=f"atom-{ai} mark",
                    )
                )
                if halo:
                    _push_halo(
                        layers,
                        circle_ring_shape(x, y, r, STROKE_PX),
                        dist=LABEL_GAP_PX,
                        cls="halo mark-halo",
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
                if halo:
                    _push_halo(
                        layers,
                        capsule_shape(x1, y1, x2, y2, 0.5 * HALO_STROKE),
                        dist=LABEL_GAP_PX,
                        cls="halo mark-halo",
                    )

    if mol_spec.annotations:
        # Place callouts / regions against the molecule occupancy grid.
        annot_grid = _mol_occupancy(layout, coords, texts)
        drawn = draw_annotations(
            mol_spec.annotations,
            atom_pos=atom_pos,
            coords=coords,
            grid=annot_grid,
        )
        layers["marks"].primitives.extend(drawn.primitives)
        if halo:
            for ink in drawn.ink:
                _push_halo(layers, ink, dist=LABEL_GAP_PX, cls="halo annot-halo")
        # Grow the viewport when annotations spill past the frame.
        if drawn.boxes:
            min_x = min(b[0] for b in drawn.boxes)
            min_y = min(b[1] for b in drawn.boxes)
            max_x = max(b[2] for b in drawn.boxes)
            max_y = max(b[3] for b in drawn.boxes)
            pad = PAD_PX * 0.25
            dx = max(0.0, pad - min_x)
            dy = max(0.0, pad - min_y)
            if dx or dy:
                _shift_layers(layers, dx, dy)
                if label_pack is not None:
                    label_pack = replace(
                        label_pack, x=label_pack.x + dx, y=label_pack.y + dy
                    )
                width += dx
                height += dy
                max_x += dx
                max_y += dy
            width = max(width, max_x + pad)
            height = max(height, max_y + pad)

    if label_pack is not None and label_pack.text:
        layers["overlay"].primitives.append(
            TextPrim(
                x=label_pack.x,
                y=label_pack.y,
                text=label_pack.text,
                font_size=label_pack.font_size,
                fill="#222",
                anchor=label_pack.anchor,
                cls="mol-label",
            )
        )
        if halo:
            ink = compile_text_shapes(
                label_pack.text,
                label_pack.x,
                label_pack.y,
                font_size=label_pack.font_size,
                anchor=label_pack.anchor,
            )
            _push_halo(layers, ink, dist=LABEL_GAP_PX, cls="halo label-halo")

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

    # Edge routes may extend slightly past node boxes (ELK root bounds).
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
        ink = None
        cls = "halo"
        if isinstance(prim, TextPrim):
            ink = compile_text_shapes(
                prim.text,
                prim.x,
                prim.y,
                font_size=prim.font_size,
                anchor=prim.anchor,
            )
            cls = "halo label-halo"
            dist = LABEL_GAP_PX
        elif isinstance(prim, PathPrim):
            ink = _ink_from_stroke_path(prim)
            ink_r = max(prim.stroke_width, STROKE_PX) * 0.5
            dist = max(LABEL_GAP_PX, 0.5 * HALO_STROKE - ink_r)
        elif isinstance(prim, CirclePrim):
            if prim.fill not in (None, "none"):
                ink = disk_shape(prim.cx, prim.cy, prim.r)
            else:
                ink = circle_ring_shape(prim.cx, prim.cy, prim.r, prim.stroke_width)
            dist = LABEL_GAP_PX
        else:
            continue
        d = halo_path_d(ink, dist)
        if not d:
            continue
        out.append(
            PathPrim(
                d=d,
                stroke="none",
                fill="#fff",
                stroke_width=0.0,
                opacity=1.0,
                cls=cls,
            )
        )
    return out

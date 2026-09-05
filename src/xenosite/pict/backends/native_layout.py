"""Native 2D placement: regular rings (CDK/RDKit style) + 120° chains.

Guide (not invention):

* Isolated / fused rings → regular *n*-gons (RDKit ``embedRing`` radius,
  CDK ``RingPlacer`` fused attachment by shared edge).
* Bridged / cage → do **not** force every SSSR face regular; seed one ring and
  place the rest with chain rules (honest hard-case path).
* Acyclic bonds → ~120° zig-zag (CDK ``AtomPlacer``).
"""

from __future__ import annotations

import math
from collections import defaultdict, deque

from xenosite.pict.backends.native_smiles import (
    ParsedMol,
    kekulize_aromatic_bonds,
    parse_organic_smiles,
)
from xenosite.pict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xenosite.pict.draw.rings import (
    Ring,
    RingAttachment,
    all_rings_can_be_regular_polygons,
    embed_regular_ring,
    find_sssr,
    ring_pair_relations,
    ring_system_components,
)

_BOND_LEN = 1.5


def _placeholder_layout(mol: ParsedMol) -> MoleculeLayout:
    atoms = [
        AtomLayout(index=a.index, element=a.element, x=0.0, y=0.0, charge=a.charge)
        for a in mol.atoms
    ]
    bonds = [
        BondLayout(index=i, begin=b.begin, end=b.end, order=b.order)
        for i, b in enumerate(mol.bonds)
    ]
    return MoleculeLayout(atoms=atoms, bonds=bonds, backend="native")


def _cycle_from_edge(ring: Ring, a: int, b: int) -> tuple[int, ...]:
    """Atom order starting at ``a`` then ``b`` around the ring."""
    atoms = list(ring.atoms)
    n = len(atoms)
    for i in range(n):
        if atoms[i] == a and atoms[(i + 1) % n] == b:
            return tuple(atoms[i:] + atoms[:i])
        if atoms[i] == a and atoms[(i - 1) % n] == b:
            out = [atoms[i]]
            j = (i - 1) % n
            while j != i:
                out.append(atoms[j])
                j = (j - 1) % n
            return tuple(out)
    raise ValueError(f"edge {a}-{b} not in ring {ring.atoms}")


def _centroid(
    atom_ids: list[int], coords: dict[int, tuple[float, float]]
) -> tuple[float, float]:
    pts = [coords[i] for i in atom_ids if i in coords]
    if not pts:
        return 0.0, 0.0
    return sum(p[0] for p in pts) / len(pts), sum(p[1] for p in pts) / len(pts)


def _seed_ring(ring: Ring, coords: dict[int, tuple[float, float]]) -> None:
    pts = embed_regular_ring(ring.size, bond_length=_BOND_LEN, start_angle=math.pi / 2)
    for idx, (x, y) in zip(ring.atoms, pts, strict=True):
        coords[idx] = (x, y)


def _place_fused_ring(
    ring: Ring,
    shared_a: int,
    shared_b: int,
    coords: dict[int, tuple[float, float]],
    existing_centroid: tuple[float, float],
) -> None:
    ordered = _cycle_from_edge(ring, shared_a, shared_b)
    n = len(ordered)
    ideal = embed_regular_ring(n, bond_length=_BOND_LEN, start_angle=0.0)
    iax, iay = ideal[0]
    ibx, iby = ideal[1]
    pax, pay = coords[shared_a]
    pbx, pby = coords[shared_b]

    idx, idy = ibx - iax, iby - iay
    pdx, pdy = pbx - pax, pby - pay
    ilen = math.hypot(idx, idy) or 1.0
    plen = math.hypot(pdx, pdy) or 1.0
    scale = plen / ilen
    rot = math.atan2(pdy, pdx) - math.atan2(idy, idx)
    cos_r, sin_r = math.cos(rot), math.sin(rot)

    def xform(x: float, y: float, *, flip: bool) -> tuple[float, float]:
        x0, y0 = x - iax, y - iay
        xr = scale * (x0 * cos_r - y0 * sin_r)
        yr = scale * (x0 * sin_r + y0 * cos_r)
        if flip:
            yr = -yr
        return pax + xr, pay + yr

    flip = False
    if n > 2:
        probe = xform(*ideal[2], flip=False)
        mid = ((pax + pbx) / 2, (pay + pby) / 2)
        to_probe = (probe[0] - mid[0], probe[1] - mid[1])
        to_exist = (existing_centroid[0] - mid[0], existing_centroid[1] - mid[1])
        if to_probe[0] * to_exist[0] + to_probe[1] * to_exist[1] > 0:
            flip = True

    for k, atom_idx in enumerate(ordered):
        if atom_idx in coords:
            continue
        coords[atom_idx] = xform(*ideal[k], flip=flip)


def _place_ring_systems(
    rings: list[Ring], coords: dict[int, tuple[float, float]]
) -> set[int]:
    placed: set[int] = set()
    if not rings:
        return placed

    relations = ring_pair_relations(rings)
    rel_by_pair: dict[frozenset[tuple[int, ...]], object] = {
        frozenset({rel.a.atoms, rel.b.atoms}): rel for rel in relations
    }

    for component in ring_system_components(rings):
        drawable = all_rings_can_be_regular_polygons(component)
        ordered = sorted(component, key=lambda r: (-r.size, r.atoms))
        seed = ordered[0]
        _seed_ring(seed, coords)
        placed.update(seed.atoms)
        if not drawable:
            continue

        pending = list(ordered[1:])
        for _ in range(len(pending) + 2):
            if not pending:
                break
            next_pending: list[Ring] = []
            progress = False
            for ring in pending:
                fused = None
                for done in ordered:
                    if done.atoms == ring.atoms:
                        continue
                    if not (set(done.atoms) & placed):
                        continue
                    rel = rel_by_pair.get(frozenset({done.atoms, ring.atoms}))
                    if rel is None or rel.kind != RingAttachment.FUSED:
                        continue
                    if len(rel.shared_atoms) != 2 or not set(rel.shared_atoms) <= placed:
                        continue
                    fused = (done, rel)
                    break
                if fused is None:
                    next_pending.append(ring)
                    continue
                done, rel = fused
                if rel.shared_bonds:
                    sa, sb = next(iter(rel.shared_bonds))
                else:
                    sa, sb = tuple(rel.shared_atoms)
                _place_fused_ring(
                    ring, sa, sb, coords, _centroid(list(done.atoms), coords)
                )
                placed.update(ring.atoms)
                progress = True
            pending = next_pending
            if not progress:
                break
    return placed


def _angle(dx: float, dy: float) -> float:
    return math.atan2(dy, dx)


def _norm_angle(a: float) -> float:
    while a <= -math.pi:
        a += 2 * math.pi
    while a > math.pi:
        a -= 2 * math.pi
    return a


def _distribute_partner_angles(
    occupied: list[float], n_new: int, *, prefer: float = math.radians(120.0)
) -> list[float]:
    """Place ``n_new`` directions in the gaps between occupied neighbor angles.

    Mirrors CDK ``AtomPlacer.distributePartners``: fill the largest free wedge(s)
    with roughly ``prefer`` (120°) spacing when degree allows.
    """
    if n_new <= 0:
        return []
    if not occupied:
        # No constraints — star from +x.
        if n_new == 1:
            return [0.0]
        return [_norm_angle(2 * math.pi * i / n_new) for i in range(n_new)]

    # One occupied neighbor: CDK chain / branch rule — place new bonds at
    # ±prefer (120°) from the existing bond, NOT opposite (180° would be linear).
    if len(occupied) == 1:
        base = occupied[0]
        if n_new == 1:
            return [_norm_angle(base + prefer)]
        # Symmetric for geminal substituents (e.g. carbonyl C with =O and -OH).
        out = []
        for i in range(n_new):
            # Alternate +prefer, -prefer, then +2prefer, ...
            sign = 1.0 if i % 2 == 0 else -1.0
            mag = prefer * ((i // 2) + 1)
            out.append(_norm_angle(base + sign * mag))
        return out

    occ = sorted(_norm_angle(a) for a in occupied)
    # Build gaps (start_angle, gap_width) sweeping CCW.
    gaps: list[tuple[float, float]] = []
    for i, a0 in enumerate(occ):
        a1 = occ[(i + 1) % len(occ)]
        width = a1 - a0
        if width <= 0:
            width += 2 * math.pi
        gaps.append((a0, width))
    # Assign slots proportional to gap size (at least one into the largest).
    gaps.sort(key=lambda g: g[1], reverse=True)
    slots = [0] * len(gaps)
    remaining = n_new
    # Prefer putting partners into gaps that can fit ~120° wedges.
    for i, (_start, width) in enumerate(gaps):
        if remaining <= 0:
            break
        # How many prefer-spaced neighbors fit in this gap (leaving margins).
        fit = max(1, int(width / prefer)) if width > prefer * 0.5 else 0
        take = min(remaining, fit if fit else (1 if i == 0 else 0))
        slots[i] = take
        remaining -= take
    if remaining > 0:
        slots[0] += remaining

    result: list[float] = []
    for (start, width), count in zip(gaps, slots, strict=True):
        if count <= 0:
            continue
        # Evenly space inside the gap, inset by half-step from edges.
        step = width / (count + 1)
        for k in range(1, count + 1):
            result.append(_norm_angle(start + step * k))
    return result


def _place_chains(mol: ParsedMol, coords: dict[int, tuple[float, float]]) -> None:
    """BFS from placed atoms; distribute partners into free angular wedges (CDK)."""
    adj: dict[int, list[int]] = defaultdict(list)
    for b in mol.bonds:
        adj[b.begin].append(b.end)
        adj[b.end].append(b.begin)

    if not coords:
        coords[0] = (0.0, 0.0)
        if adj[0]:
            coords[adj[0][0]] = (_BOND_LEN, 0.0)

    queue = deque(sorted(coords.keys()))
    seen = set(coords.keys())

    while queue:
        u = queue.popleft()
        unplaced = [v for v in adj[u] if v not in coords]
        if not unplaced:
            continue
        ux, uy = coords[u]
        occupied = [
            _angle(coords[v][0] - ux, coords[v][1] - uy) for v in adj[u] if v in coords
        ]
        # Chain zig-zag bias: if exactly one occupied neighbor, prefer ±120° bond
        # angle (CDK linear chain) by seeding a phantom occupied opposite so the
        # free wedge favors the turn — handled naturally when occupied has 1 entry
        # (gap is 360°, distributePartners places at prefer spacing).
        angles = _distribute_partner_angles(occupied, len(unplaced))
        # Stable pairing: sort unplaced by index for determinism.
        for v, ang in zip(sorted(unplaced), angles, strict=False):
            coords[v] = (ux + math.cos(ang) * _BOND_LEN, uy + math.sin(ang) * _BOND_LEN)
            if v not in seen:
                seen.add(v)
                queue.append(v)

    for a in mol.atoms:
        if a.index not in coords:
            coords[a.index] = (float(a.index) * _BOND_LEN, 0.0)


def layout_parsed(mol: ParsedMol, *, mol_id: str | None = None) -> MoleculeLayout:
    kekulize_aromatic_bonds(mol)
    stub = _placeholder_layout(mol)
    rings = find_sssr(stub)
    coords: dict[int, tuple[float, float]] = {}
    _place_ring_systems(rings, coords)
    _place_chains(mol, coords)

    atoms = [
        AtomLayout(
            index=a.index,
            element=a.element,
            x=coords[a.index][0],
            y=coords[a.index][1],
            charge=a.charge,
            label=None if a.element == "C" and a.charge == 0 else a.element,
        )
        for a in mol.atoms
    ]
    bonds = [
        BondLayout(index=i, begin=b.begin, end=b.end, order=float(b.order))
        for i, b in enumerate(mol.bonds)
    ]
    warnings = list(mol.warnings)
    if rings and not all_rings_can_be_regular_polygons(rings):
        warnings.append(
            "bridged/cage ring system: native does not force all SSSR faces regular"
        )
    warnings.append(
        "native layout is experimental; indigo remains the transitional default"
    )
    return MoleculeLayout(
        id=mol_id,
        atoms=atoms,
        bonds=bonds,
        backend="native",
        warnings=warnings,
    )


def layout_smiles(smiles: str, *, mol_id: str | None = None) -> MoleculeLayout:
    return layout_parsed(parse_organic_smiles(smiles), mol_id=mol_id)

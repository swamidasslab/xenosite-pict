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
    ParsedBond,
    ParsedMol,
    atom_display_label,
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
    # Edge length² for reflecting across the *shared bond* (not world X).
    # Negating local Y after rotation only mirrors across world-horizontal and
    # moves already-placed shared atoms — that broke anthracene fusion.
    edge2 = pdx * pdx + pdy * pdy or 1.0

    def xform(x: float, y: float, *, flip: bool) -> tuple[float, float]:
        x0, y0 = x - iax, y - iay
        xr = scale * (x0 * cos_r - y0 * sin_r)
        yr = scale * (x0 * sin_r + y0 * cos_r)
        if flip:
            # Reflect offset (xr, yr) across shared-edge direction (CDK fuse side).
            proj = (xr * pdx + yr * pdy) / edge2
            xr, yr = 2.0 * proj * pdx - xr, 2.0 * proj * pdy - yr
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
        # Seed the most-connected fused ring (CDK-ish), then larger faces.
        comp_atoms = {r.atoms for r in component}
        fuse_degree: dict[tuple[int, ...], int] = defaultdict(int)
        for rel in relations:
            if rel.kind != RingAttachment.FUSED:
                continue
            if rel.a.atoms in comp_atoms and rel.b.atoms in comp_atoms:
                fuse_degree[rel.a.atoms] += 1
                fuse_degree[rel.b.atoms] += 1
        ordered = sorted(
            component,
            key=lambda r: (-fuse_degree[r.atoms], -r.size, r.atoms),
        )
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
    """BFS from placed atoms; distribute partners into free angular wedges (CDK).

    Acyclic molecules: seed the **longest chain** as a 120° zig-zag first
    (CDK ``AtomPlacer.placeLinearChain``), then grow branches from it.
    """
    adj: dict[int, list[int]] = defaultdict(list)
    for b in mol.bonds:
        adj[b.begin].append(b.end)
        adj[b.end].append(b.begin)

    if not coords:
        _seed_longest_chain(mol, coords, adj)

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


def _longest_path(adj: dict[int, list[int]], n_atoms: int) -> list[int]:
    """Longest simple path in an undirected tree/graph (BFS diameter for trees).

    For small molecules we BFS from every node; gallery sizes stay tiny.
    """
    if n_atoms == 0:
        return []
    if n_atoms == 1:
        return [0]

    def farthest(start: int) -> tuple[int, list[int]]:
        prev: dict[int, int | None] = {start: None}
        q = deque([start])
        last = start
        while q:
            u = q.popleft()
            last = u
            for v in adj[u]:
                if v not in prev:
                    prev[v] = u
                    q.append(v)
        path = []
        cur: int | None = last
        while cur is not None:
            path.append(cur)
            cur = prev[cur]
        path.reverse()
        return last, path

    # Two BFS (tree diameter). For graphs with cycles this is a good heuristic.
    end1, _ = farthest(0)
    end2, path = farthest(end1)
    # Also try other starts if a longer path exists (cycles / disconnected).
    best = path
    for start in range(n_atoms):
        _, p = farthest(start)
        if len(p) > len(best):
            best = p
        elif len(p) == len(best) and p < best:
            best = p  # deterministic tie-break
    return best


def _seed_longest_chain(
    mol: ParsedMol,
    coords: dict[int, tuple[float, float]],
    adj: dict[int, list[int]],
) -> None:
    """Place the longest chain as a 120° zig-zag (CDK placeLinearChain)."""
    path = _longest_path(adj, len(mol.atoms))
    if len(path) < 2:
        return
    coords[path[0]] = (0.0, 0.0)
    coords[path[1]] = (_BOND_LEN, 0.0)
    # Alternate ±60° from the previous bond direction → 120° bond angles.
    sign = 1.0
    for i in range(2, len(path)):
        ax, ay = coords[path[i - 2]]
        bx, by = coords[path[i - 1]]
        prev_ang = _angle(bx - ax, by - ay)
        # Turn by 60° from collinear (= 120° bond angle at path[i-1]).
        ang = prev_ang + sign * math.radians(60.0)
        coords[path[i]] = (
            bx + math.cos(ang) * _BOND_LEN,
            by + math.sin(ang) * _BOND_LEN,
        )
        sign = -sign


def _stereo_parity_from_db(bond: ParsedBond, db_atom: int) -> bool | None:
    """True if substituent is on the '+' side looking out from the double-bond atom.

    OpenSMILES: ``/`` from db→substituent is '+'; written substituent→db flips.
    """
    if bond.stereo not in {"/", "\\"}:
        return None
    if bond.begin == db_atom:
        return bond.stereo == "/"
    if bond.end == db_atom:
        return bond.stereo == "\\"
    return None


def _connected_component(
    start: int, blocked: set[int], adj: dict[int, list[int]]
) -> set[int]:
    """Atoms reachable from ``start`` without crossing ``blocked``."""
    out = {start}
    q = deque([start])
    while q:
        u = q.popleft()
        for v in adj[u]:
            if v in blocked or v in out:
                continue
            out.add(v)
            q.append(v)
    return out


def _reflect_across_axis(
    coords: dict[int, tuple[float, float]],
    atoms: set[int],
    ax: float,
    ay: float,
    bx: float,
    by: float,
) -> None:
    """Reflect ``atoms`` across the line through (ax,ay)–(bx,by)."""
    dx, dy = bx - ax, by - ay
    length2 = dx * dx + dy * dy or 1.0
    for i in atoms:
        px, py = coords[i]
        # Vector from A to P
        vx, vy = px - ax, py - ay
        proj = (vx * dx + vy * dy) / length2
        fx, fy = ax + proj * dx, ay + proj * dy
        coords[i] = (2 * fx - px, 2 * fy - py)


def _enforce_ez_stereo(mol: ParsedMol, coords: dict[int, tuple[float, float]]) -> None:
    """Flip one substituent side of a double bond to match OpenSMILES E/Z.

    Guide: OpenSMILES ``/`` ``\\`` — same relative parity → opposite sides (trans);
    opposite parity → same side (cis). Layout places freely first; this post-pass
    mirrors the smaller substituent tree across the double-bond axis when needed.
    """
    adj: dict[int, list[int]] = defaultdict(list)
    bonds_by_pair: dict[frozenset[int], ParsedBond] = {}
    for b in mol.bonds:
        adj[b.begin].append(b.end)
        adj[b.end].append(b.begin)
        bonds_by_pair[frozenset({b.begin, b.end})] = b

    for db in mol.bonds:
        if db.order < 1.5 or db.order >= 2.5:
            continue
        a, b = db.begin, db.end
        # Find one stereo-marked substituent on each end (prefer heavy, non-H).
        sub_a = sub_b = None
        parity_a = parity_b = None
        for nbr in adj[a]:
            if nbr == b:
                continue
            pb = bonds_by_pair.get(frozenset({a, nbr}))
            if pb is None:
                continue
            p = _stereo_parity_from_db(pb, a)
            if p is not None:
                sub_a, parity_a = nbr, p
                break
        for nbr in adj[b]:
            if nbr == a:
                continue
            pb = bonds_by_pair.get(frozenset({b, nbr}))
            if pb is None:
                continue
            p = _stereo_parity_from_db(pb, b)
            if p is not None:
                sub_b, parity_b = nbr, p
                break
        if sub_a is None or sub_b is None or parity_a is None or parity_b is None:
            continue

        # Same parity → cis (same side); different → trans (opposite sides).
        want_same_side = parity_a == parity_b

        ax, ay = coords[a]
        bx, by = coords[b]
        # Cross product sign: (sub-a) × (db vector) vs (sub-b) × (db vector).
        dx, dy = bx - ax, by - ay
        sax, say = coords[sub_a][0] - ax, coords[sub_a][1] - ay
        sbx, sby = coords[sub_b][0] - bx, coords[sub_b][1] - by
        cross_a = dx * say - dy * sax
        cross_b = dx * sby - dy * sbx
        if abs(cross_a) < 1e-9 or abs(cross_b) < 1e-9:
            continue
        have_same_side = (cross_a > 0) == (cross_b > 0)
        if have_same_side == want_same_side:
            continue

        # Flip the smaller substituent tree across the double-bond axis.
        comp_a = _connected_component(sub_a, {a, b}, adj)
        comp_b = _connected_component(sub_b, {a, b}, adj)
        flip = comp_b if len(comp_b) <= len(comp_a) else comp_a
        _reflect_across_axis(coords, flip, ax, ay, bx, by)


def _clash_score(
    coords: dict[int, tuple[float, float]],
    bonded: set[frozenset[int]],
    *,
    min_sep: float,
) -> float:
    """Sum of penetration depths for non-bonded pairs closer than ``min_sep``."""
    ids = list(coords)
    score = 0.0
    for i, a in enumerate(ids):
        ax, ay = coords[a]
        for b in ids[i + 1 :]:
            if frozenset({a, b}) in bonded:
                continue
            d = math.hypot(coords[b][0] - ax, coords[b][1] - ay)
            if d < min_sep:
                score += min_sep - d
    return score


def _mitigate_terminal_collisions(
    mol: ParsedMol, coords: dict[int, tuple[float, float]]
) -> None:
    """Flip terminal substituents across their attachment bond if they clash.

    Guide: RDKit depictor collision flips / CDK openAngles — try the alternate
    side of the parent before accepting an overlap. Only moves degree-1 atoms
    so ring geometry stays intact.
    """
    adj: dict[int, list[int]] = defaultdict(list)
    bonded: set[frozenset[int]] = set()
    for b in mol.bonds:
        adj[b.begin].append(b.end)
        adj[b.end].append(b.begin)
        bonded.add(frozenset({b.begin, b.end}))

    min_sep = _BOND_LEN * 0.85
    terminals = [
        a.index
        for a in mol.atoms
        if len(adj[a.index]) == 1 and a.index in coords
    ]
    for t in terminals:
        parent = adj[t][0]
        px, py = coords[parent]
        # Alternate angle: reflect current terminal across each occupied
        # parent→neighbor ray and keep the placement with lowest clash.
        occupied = [
            _angle(coords[v][0] - px, coords[v][1] - py)
            for v in adj[parent]
            if v != t and v in coords
        ]
        if not occupied:
            continue
        candidates = [coords[t]]
        for occ in occupied:
            # Reflect the parent→terminal angle across the occupied ray.
            cur = _angle(coords[t][0] - px, coords[t][1] - py)
            mirrored = _norm_angle(2 * occ - cur)
            candidates.append(
                (px + math.cos(mirrored) * _BOND_LEN, py + math.sin(mirrored) * _BOND_LEN)
            )
        # Also try ±120° from the mean occupied direction (exterior preference).
        mean_occ = math.atan2(
            sum(math.sin(a) for a in occupied) / len(occupied),
            sum(math.cos(a) for a in occupied) / len(occupied),
        )
        for delta in (math.radians(120.0), -math.radians(120.0)):
            ang = _norm_angle(mean_occ + delta)
            candidates.append(
                (px + math.cos(ang) * _BOND_LEN, py + math.sin(ang) * _BOND_LEN)
            )

        best = coords[t]
        best_score = _clash_score(coords, bonded, min_sep=min_sep)
        for cand in candidates[1:]:
            trial = dict(coords)
            trial[t] = cand
            score = _clash_score(trial, bonded, min_sep=min_sep)
            if score < best_score - 1e-9:
                best_score = score
                best = cand
        coords[t] = best


def _degree_map(mol: ParsedMol) -> dict[int, int]:
    deg: dict[int, int] = defaultdict(int)
    for b in mol.bonds:
        deg[b.begin] += 1
        deg[b.end] += 1
    return deg


def _assign_tetrahedral_wedges(
    mol: ParsedMol,
    coords: dict[int, tuple[float, float]],
    bonds: list[BondLayout],
) -> None:
    """Assign up/down wedges from OpenSMILES ``@``/``@@`` (lab-quality).

    Guide: RDKit ``WedgeMolBonds`` — thin end at stereocenter (``begin``).
    Parity: compute 2D CCW order of heavy neighbors; flip wedge sense so the
    depicted configuration matches SMILES ``@`` (anticlockwise) / ``@@``
    (clockwise) when looking from the first SMILES neighbor. Not full CIP —
    enough to exercise the draw path on native coords.
    """
    adj: dict[int, list[int]] = defaultdict(list)
    bond_by_pair: dict[frozenset[int], BondLayout] = {}
    for b in bonds:
        adj[b.begin].append(b.end)
        adj[b.end].append(b.begin)
        bond_by_pair[frozenset({b.begin, b.end})] = b

    for atom in mol.atoms:
        if atom.tetrahedral not in {"@", "@@"}:
            continue
        nbrs = list(adj[atom.index])
        if len(nbrs) < 3:
            continue
        # Prefer wedging a terminal (or lowest-degree) substituent — RDKit-ish.
        wedge_end = min(nbrs, key=lambda v: (len(adj[v]), v))
        bl = bond_by_pair.get(frozenset({atom.index, wedge_end}))
        if bl is None or bl.order >= 1.5:
            continue
        # Orient bond: begin = stereocenter.
        bl.begin, bl.end = atom.index, wedge_end

        cx, cy = coords[atom.index]
        ordered = sorted(
            nbrs,
            key=lambda v: math.atan2(coords[v][1] - cy, coords[v][0] - cx),
        )
        # Signed area of triangle (n0,n1,n2) around center → 2D winding of neighbors.
        def _ccw(a: int, b: int, c: int) -> float:
            ax, ay = coords[a][0] - cx, coords[a][1] - cy
            bx, by = coords[b][0] - cx, coords[b][1] - cy
            dx, dy = coords[c][0] - cx, coords[c][1] - cy
            # Project: sum of cross products in angular order.
            return ax * by - ay * bx + bx * dy - by * dx + dx * ay - dy * ax

        if len(ordered) >= 3:
            winding = _ccw(ordered[0], ordered[1], ordered[2])
        else:
            winding = 1.0
        # SMILES @ = anticlockwise from first neighbor; @@ = clockwise.
        # With wedge-up meaning wedge_end above the paper, a positive 2D CCW
        # winding of the other ligands matches @ when the wedged atom is
        # treated as "in front". Flip if mismatch.
        want_ccw = atom.tetrahedral == "@"
        have_ccw = winding > 0
        if want_ccw == have_ccw:
            bl.stereo = "up"
        else:
            bl.stereo = "down"


def layout_parsed(mol: ParsedMol, *, mol_id: str | None = None) -> MoleculeLayout:
    kekulize_aromatic_bonds(mol)
    stub = _placeholder_layout(mol)
    rings = find_sssr(stub)
    coords: dict[int, tuple[float, float]] = {}
    _place_ring_systems(rings, coords)
    _place_chains(mol, coords)
    _mitigate_terminal_collisions(mol, coords)
    # After collision flips — E/Z from OpenSMILES ``/`` ``\\`` must win.
    _enforce_ez_stereo(mol, coords)

    deg = _degree_map(mol)
    atoms = [
        AtomLayout(
            index=a.index,
            element=a.element,
            x=coords[a.index][0],
            y=coords[a.index][1],
            charge=a.charge,
            radical=a.radical,
            label=atom_display_label(a, deg[a.index]),
        )
        for a in mol.atoms
    ]
    bonds = [
        BondLayout(index=i, begin=b.begin, end=b.end, order=float(b.order))
        for i, b in enumerate(mol.bonds)
    ]
    _assign_tetrahedral_wedges(mol, coords, bonds)
    warnings = list(mol.warnings)
    if rings and not all_rings_can_be_regular_polygons(rings):
        warnings.append(
            "bridged/cage ring system: native does not force all SSSR faces regular"
        )
    if any(a.tetrahedral for a in mol.atoms):
        warnings.append(
            "native tetrahedral wedges are SMILES-parity heuristics, not full CIP"
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

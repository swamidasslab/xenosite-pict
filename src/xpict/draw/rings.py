"""SSSR rings and depiction helpers grounded in established depictors.

Coordinate generation (regular polygons, fused stitching, bridged arcs) belongs
to the layout backend — not a post-hoc snap in the SVG drawer. Quality engines:

* **CDK** ``StructureDiagramGenerator`` / ``RingPlacer``
  (``tool/sdg/.../RingPlacer.java``): place an isolated ring as a regular
  *n*-gon; then attach neighbors by shared-atom count —
  **FUSED** = 2 atoms + 1 bond, **SPIRO** = 1 atom, **BRIDGED** = >2 atoms.
  Bridged placement walks the *unplaced* atoms along an arc
  (``tStep = tRemain / (numUnplaced+1)``); the whole ring is **not** forced
  to a perfect regular polygon.
* **RDKit** ``EmbeddedFrag::embedFusedRings`` / ``embedRing``
  (``Depictor/EmbeddedFrag.cpp``, ``DepictUtils.cpp``): each SSSR ring is
  first embedded alone as a regular polygon
  (``al = BOND_LEN / sqrt(2*(1-cos(2π/n)))``), then fused systems are
  stitched by shared atoms. Complex / cage systems prefer **ring templates**
  or **CoordGen**; when stitching conflicts (bridged cages), neighbors are
  left alone rather than over-constrained into regular *n*-gons.
* **Indigo** ``layout()`` / ``iterateSSSR``: publication layout owns coords;
  we consume SSSR for drawing decisions.

This module therefore:

1. Computes a practical SSSR (shortest cycle through each bond).
2. Classifies pairwise ring attachments with CDK's FUSED/SPIRO/BRIDGED rules.
3. Flags ring systems that cannot be drawn as all-regular polygons (hard cases).
4. Supplies ring-interior bond normals and aromatic-circle geometry for the
   drawer (Kekulé offsets / optional aromatic circle *after* layout).

Do **not** independently lerp every SSSR ring toward a regular polygon after
the backend has laid out a bridged cage — that fights RDKit/CDK/Indigo.
"""

from __future__ import annotations

import math
from collections import defaultdict, deque
from dataclasses import dataclass
from enum import Enum

from xpict.contracts.layout import MoleculeLayout


def bond_key(a: int, b: int) -> tuple[int, int]:
    return (a, b) if a < b else (b, a)


@dataclass(frozen=True)
class Ring:
    """One SSSR ring as an ordered atom cycle (atom indices)."""

    atoms: tuple[int, ...]

    @property
    def size(self) -> int:
        return len(self.atoms)

    @property
    def bonds(self) -> frozenset[tuple[int, int]]:
        n = len(self.atoms)
        return frozenset(
            bond_key(self.atoms[i], self.atoms[(i + 1) % n]) for i in range(n)
        )


class RingAttachment(str, Enum):
    """CDK ``RingPlacer`` connection kinds (by shared-atom / shared-bond count)."""

    ISOLATED = "isolated"
    FUSED = "fused"  # 2 atoms, 1 bond
    SPIRO = "spiro"  # 1 atom, 0 bonds
    BRIDGED = "bridged"  # >2 atoms (and typically >1 shared bond)


@dataclass(frozen=True)
class RingPairRelation:
    """How two SSSR rings share atoms/bonds (CDK placeRing dispatch inputs)."""

    a: Ring
    b: Ring
    shared_atoms: frozenset[int]
    shared_bonds: frozenset[tuple[int, int]]
    kind: RingAttachment


def _adjacency(layout: MoleculeLayout) -> dict[int, list[int]]:
    adj: dict[int, list[int]] = {a.index: [] for a in layout.atoms}
    for b in layout.bonds:
        adj[b.begin].append(b.end)
        adj[b.end].append(b.begin)
    return adj


def _shortest_cycle_through_bond(
    adj: dict[int, list[int]], u: int, v: int, max_size: int = 8
) -> list[int] | None:
    """Shortest cycle containing edge u–v (remove the edge, BFS u→v).

    Same practical idea as Figueras-style SSSR candidates used in many
    cheminformatics stacks: for each ring bond, the shortest return path.
    """
    prev: dict[int, int | None] = {u: None}
    q: deque[int] = deque([u])
    found = False
    while q and not found:
        cur = q.popleft()
        depth = 0
        t = cur
        while t is not None and t != u:
            depth += 1
            t = prev[t]
        if depth > max_size - 1:
            continue
        for nxt in adj[cur]:
            if (cur == u and nxt == v) or (cur == v and nxt == u):
                continue
            if nxt == v and cur != u:
                prev[v] = cur
                found = True
                break
            if nxt in prev:
                continue
            prev[nxt] = cur
            q.append(nxt)
    if not found or v not in prev:
        return None
    path = [v]
    cur: int | None = v
    while cur != u:
        cur = prev[cur]
        if cur is None:
            return None
        path.append(cur)
        if len(path) > max_size:
            return None
    path.reverse()
    if len(path) < 3 or len(path) > max_size:
        return None
    return path


def find_sssr(layout: MoleculeLayout, *, max_size: int = 8) -> list[Ring]:
    """Practical SSSR for depiction: unique shortest cycles through each bond.

    Not bit-unique like Berger *et al.*; for cages several SSSRs exist
    (CDK book notes adamantane). Depiction only needs a small-ring basis.
    """
    if not layout.bonds:
        return []
    adj = _adjacency(layout)
    by_bonds: dict[frozenset[tuple[int, int]], Ring] = {}
    for b in layout.bonds:
        cyc = _shortest_cycle_through_bond(adj, b.begin, b.end, max_size=max_size)
        if cyc is None:
            continue
        ring = Ring(atoms=tuple(cyc))
        prev = by_bonds.get(ring.bonds)
        if prev is None or ring.size < prev.size:
            by_bonds[ring.bonds] = ring
    rings = sorted(by_bonds.values(), key=lambda r: (r.size, r.atoms))
    # Greedy cover: prefer smaller rings; keep a ring if it adds a new bond.
    kept: list[Ring] = []
    covered: set[tuple[int, int]] = set()
    for ring in rings:
        if ring.bonds - covered:
            kept.append(ring)
            covered |= ring.bonds
    return kept or rings


def classify_ring_pair(a: Ring, b: Ring) -> RingPairRelation | None:
    """CDK ``RingPlacer.placeRing`` dispatch on shared atoms/bonds."""
    if a.bonds == b.bonds:
        return None
    shared_atoms = frozenset(a.atoms) & frozenset(b.atoms)
    shared_bonds = a.bonds & b.bonds
    n_atoms = len(shared_atoms)
    n_bonds = len(shared_bonds)
    if n_atoms == 0:
        return None
    if n_atoms > 2 and n_bonds > 1:
        kind = RingAttachment.BRIDGED
    elif n_atoms == 2 and n_bonds == 1:
        kind = RingAttachment.FUSED
    elif n_atoms == 1 and n_bonds == 0:
        kind = RingAttachment.SPIRO
    elif n_atoms > 2:
        # Shared path without counting as classic fused edge (still bridged).
        kind = RingAttachment.BRIDGED
    elif n_atoms == 2:
        kind = RingAttachment.FUSED
    else:
        kind = RingAttachment.SPIRO
    return RingPairRelation(
        a=a, b=b, shared_atoms=shared_atoms, shared_bonds=shared_bonds, kind=kind
    )


def ring_pair_relations(rings: list[Ring]) -> list[RingPairRelation]:
    out: list[RingPairRelation] = []
    for i, a in enumerate(rings):
        for b in rings[i + 1 :]:
            rel = classify_ring_pair(a, b)
            if rel is not None:
                out.append(rel)
    return out


def ring_system_components(rings: list[Ring]) -> list[list[Ring]]:
    """Connected components of rings that share ≥1 atom (fused systems)."""
    if not rings:
        return []
    parent = list(range(len(rings)))

    def find(i: int) -> int:
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    def union(i: int, j: int) -> None:
        ri, rj = find(i), find(j)
        if ri != rj:
            parent[rj] = ri

    for i, a in enumerate(rings):
        sa = frozenset(a.atoms)
        for j in range(i + 1, len(rings)):
            if sa & frozenset(rings[j].atoms):
                union(i, j)
    groups: dict[int, list[Ring]] = defaultdict(list)
    for i, ring in enumerate(rings):
        groups[find(i)].append(ring)
    return list(groups.values())


def ring_system_has_bridge(rings: list[Ring]) -> bool:
    """True if any pair in this fused component is BRIDGED (CDK sense)."""
    return any(rel.kind == RingAttachment.BRIDGED for rel in ring_pair_relations(rings))


def ring_system_is_overconstrained(rings: list[Ring]) -> bool:
    """True when sequential regular-*n*-gon placement cannot satisfy the system.

    Two failure modes that quality depictors handle specially (not by
    independently snapping every SSSR face):

    1. **CDK BRIDGED** pair — shared atoms > 2 (norbornane, …).
       ``RingPlacer.placeBridgedRing`` places only the *unplaced* atoms on an
       arc; the whole ring is not a perfect regular polygon.
    2. **Cage / polyhedral** systems — an atom belongs to ≥3 SSSR rings
       (cubane, prismane, …). Every *pairwise* attachment may still look
       FUSED (shared edge), yet a planar embedding cannot make every face a
       regular polygon. RDKit falls back to ring templates / CoordGen here
       (``EmbeddedFrag::matchToTemplate``).
    """
    if ring_system_has_bridge(rings):
        return True
    atom_ring_count: dict[int, int] = defaultdict(int)
    for ring in rings:
        for atom in ring.atoms:
            atom_ring_count[atom] += 1
    return any(count >= 3 for count in atom_ring_count.values())


def all_rings_can_be_regular_polygons(rings: list[Ring]) -> bool:
    """Whether every ring in the molecule can be a perfect regular *n*-gon.

    Mirrors depictor practice:

    * Isolated rings → yes (CDK ``placeRing`` / RDKit ``embedRing``).
    * Pure edge-fused + spiro systems → yes *as a sequential construction*
      (each new ring is a regular polygon attached at a fusion bond or spiro
      atom). Naphthalene, indole, biphenyl (separate systems) qualify.
    * **BRIDGED** attachments or cage atoms in ≥3 rings → **no**.
      CDK uses ``placeBridgedRing``; RDKit uses templates/CoordGen or accepts
      non-regular stitching. Forcing every SSSR ring to a regular polygon
      over-constrains the embedding — do not post-hoc lerp coords.
    """
    if not rings:
        return True
    for component in ring_system_components(rings):
        if ring_system_is_overconstrained(component):
            return False
    return True


def regular_polygon_radius(n: int, bond_length: float) -> float:
    """RDKit ``embedRing`` / CDK ``getNativeRingRadius`` arm length.

    ``R = b / sqrt(2 * (1 - cos(2π/n)))``.
    """
    if n < 3:
        raise ValueError("n must be >= 3")
    ang = 2.0 * math.pi / n
    return bond_length / math.sqrt(2.0 * (1.0 - math.cos(ang)))


def embed_regular_ring(
    n: int, *, bond_length: float = 1.0, start_angle: float = 0.0
) -> list[tuple[float, float]]:
    """Isolated-ring coords as in RDKit ``DepictUtils::embedRing`` (origin-centered)."""
    radius = regular_polygon_radius(n, bond_length)
    return [
        (
            radius * math.cos(start_angle + 2.0 * math.pi * i / n),
            radius * math.sin(start_angle + 2.0 * math.pi * i / n),
        )
        for i in range(n)
    ]


def ring_centroid(
    ring: Ring, coords_by_index: dict[int, tuple[float, float]]
) -> tuple[float, float]:
    xs = [coords_by_index[i][0] for i in ring.atoms]
    ys = [coords_by_index[i][1] for i in ring.atoms]
    return sum(xs) / len(xs), sum(ys) / len(ys)


def bond_interior_normals(
    rings: list[Ring], coords_by_index: dict[int, tuple[float, float]]
) -> dict[tuple[int, int], tuple[float, float]]:
    """Unit normals for ring bonds pointing toward the owning (smallest) ring centroid."""
    normals: dict[tuple[int, int], tuple[float, float]] = {}
    for ring in sorted(rings, key=lambda r: r.size):
        cx, cy = ring_centroid(ring, coords_by_index)
        n = ring.size
        for i in range(n):
            a, b = ring.atoms[i], ring.atoms[(i + 1) % n]
            key = bond_key(a, b)
            if key in normals:
                continue
            x1, y1 = coords_by_index[a]
            x2, y2 = coords_by_index[b]
            dx, dy = x2 - x1, y2 - y1
            length = math.hypot(dx, dy) or 1.0
            nx, ny = -dy / length, dx / length
            mx, my = (x1 + x2) / 2, (y1 + y2) / 2
            if (cx - mx) * nx + (cy - my) * ny < 0:
                nx, ny = -nx, -ny
            normals[key] = (nx, ny)
    return normals


# Catalog of molecules where not every SSSR face can be a regular polygon.
# Used by tests and docs; SMILES are canonical-ish references.
HARD_RING_CASES: list[dict[str, str]] = [
    {
        "id": "norbornane",
        "smiles": "C1CC2CCC1C2",
        "notes": "bridged [2.2.1]; two SSSR rings share >2 atoms (CDK BRIDGED)",
    },
    {
        "id": "norbornadiene",
        "smiles": "C1C2C=CC1C=C2",
        "notes": "unsaturated bridged; same attachment class as norbornane",
    },
    {
        "id": "bicyclo222_octane",
        "smiles": "C1CC2CCC1CC2",
        "notes": "bridged [2.2.2]; cannot regularize both faces independently",
    },
    {
        "id": "cubane",
        "smiles": "C12C3C4C1C5C2C3C45",
        "notes": "cage; RDKit/CoordGen use templates — not all faces regular in 2D",
    },
    {
        "id": "adamantane",
        "smiles": "C1C2CC3CC1CC(C2)C3",
        "notes": "diamondoid cage; multiple non-unique SSSRs (CDK book)",
    },
    {
        "id": "barrelene",
        "smiles": "C1=CC2C=CC1C=C2",
        "notes": "bridged triene; overconstrained regular faces",
    },
    {
        "id": "twistane",
        "smiles": "C1CC2CCC3C(C1)C2CC3",
        "notes": "twisted cage; bridged ring system",
    },
]

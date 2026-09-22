"""Align later molecules onto the first molecule's coordinate frame.

Two implementations share one interface (``Aligner``):

* **Template** (RDKit, when installed): matched atoms are fixed on the
  reference coordinates and the rest of the molecule is depicted around
  them. This is the correct 2D alignment.
* **Rigid** (always available): each molecule is depicted on its own, then
  rotated and translated so the matched atoms overlap as well as a rigid
  move allows. Indigo and native cannot do the template step.

The correspondence we lock is the largest set of atoms that already sit on
the reference (near-zero rigid error). One extra atom that does not fit is
not worth dragging that set off the reference. The first substructure hit
is an arbitrary automorphism, so every embedding is scored before anything
is fixed. If nothing is near zero, the largest subgraph is still used so a
template can redraw a different shape.

``align_layouts`` picks RDKit when it imports, and falls back to rigid if
template depiction fails or RDKit is absent. RDKit is also the preferred
**layout** backend for 2D coords (``backends.rdkit_layout``); Indigo layouts
should use rigid / fake align only.
"""

from __future__ import annotations

import math
from collections import defaultdict
from collections.abc import Sequence
from typing import Protocol

from xpict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xpict.future.spec import MoleculeSpec


def _element_key(el: str) -> str:
    if el == "*" or el.startswith("R") or el.startswith("_"):
        return "*"
    return el


def _adj(layout: MoleculeLayout) -> dict[int, dict[int, float]]:
    adj: dict[int, dict[int, float]] = defaultdict(dict)
    for b in layout.bonds:
        order = float(b.order or 1.0)
        adj[b.begin][b.end] = order
        adj[b.end][b.begin] = order
    return adj


def _order_ok(a: float, b: float, *, ring: bool = False) -> bool:
    if abs(a - b) < 0.1:
        return True
    lo, hi = (a, b) if a <= b else (b, a)
    # Kekule single/double in a ring are the same aromatic bond. Requiring
    # them to match freezes one ring rotation — a local correspondence.
    if ring and lo >= 0.9 and hi <= 2.1:
        return True
    return lo >= 1.4 and hi <= 2.1


def _ring_edges(adj: dict[int, dict[int, float]]) -> set[frozenset[int]]:
    """Edges that still connect their endpoints after the edge is removed."""
    edges: list[frozenset[int]] = []
    seen: set[frozenset[int]] = set()
    for a, nbrs in adj.items():
        for b in nbrs:
            edge = frozenset((a, b))
            if len(edge) == 2 and edge not in seen:
                seen.add(edge)
                edges.append(edge)
    ring: set[frozenset[int]] = set()
    for edge in edges:
        start, goal = tuple(edge)
        stack = [start]
        visited = {start}
        while stack:
            u = stack.pop()
            if u == goal:
                ring.add(edge)
                break
            for v in adj[u]:
                if v in visited or frozenset((u, v)) == edge:
                    continue
                visited.add(v)
                stack.append(v)
    return ring


def _mean_bond_length(layout: MoleculeLayout) -> float:
    by_index = {a.index: a for a in layout.atoms}
    lengths: list[float] = []
    for bond in layout.bonds:
        a = by_index.get(bond.begin)
        b = by_index.get(bond.end)
        if a is None or b is None:
            continue
        lengths.append(math.hypot(a.x - b.x, a.y - b.y))
    if not lengths:
        return 1.0
    mean = sum(lengths) / len(lengths)
    return mean if mean > 1e-6 else 1.0


def _atom_errors(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    mapping: dict[int, int],
) -> list[tuple[int, float]]:
    """Per-atom error after the best rigid move. Pairs are ``(other_index, error)``."""
    ref_at = {a.index: a for a in ref.atoms}
    oth_at = {a.index: a for a in other.atoms}
    pairs = [(o, r) for o, r in mapping.items() if o in oth_at and r in ref_at]
    if not pairs:
        return []
    src = [(oth_at[o].x, oth_at[o].y) for o, _r in pairs]
    dst = [(ref_at[r].x, ref_at[r].y) for _o, r in pairs]
    cos_r, sin_r, tx, ty, det = _kabsch_2d(src, dst)
    errors: list[tuple[int, float]] = []
    for (o, _r), (x, y), (u, v) in zip(pairs, src, dst, strict=True):
        if det >= 0:
            xx = cos_r * x - sin_r * y + tx
            yy = sin_r * x + cos_r * y + ty
        else:
            xx = cos_r * x + sin_r * y + tx
            yy = sin_r * x - cos_r * y + ty
        errors.append((o, math.hypot(xx - u, yy - v)))
    return errors


def _mapping_rmsd(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    mapping: dict[int, int],
) -> float:
    """Kabsch RMSD of this correspondence, using the layouts' current coordinates."""
    errors = _atom_errors(ref, other, mapping)
    if not errors:
        return math.inf
    return math.sqrt(sum(err * err for _o, err in errors) / len(errors))


# An atom this far from its partner, in units of the reference bond, is not
# "already there". Looser than depiction noise, tighter than a wrong match.
_NEAR_ZERO_BOND_FRAC = 0.05


def _near_zero_core(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    mapping: dict[int, int],
    *,
    min_size: int,
) -> dict[int, int] | None:
    """Drop atoms that spoil a rigid fit until the rest sit on the reference.

    Kabsch on the whole map will slide every atom to buy one bad pair. That
    pair is removed and the fit is redone. Returns None when no subset of
    ``min_size`` is near zero (the shapes really differ).
    """
    tol = _NEAR_ZERO_BOND_FRAC * _mean_bond_length(ref)
    current = dict(mapping)
    while len(current) >= min_size:
        errors = _atom_errors(ref, other, current)
        if len(errors) < min_size:
            return None
        if max(err for _o, err in errors) <= tol:
            return {o: current[o] for o, _err in errors}
        if len(current) == min_size:
            return None
        worst = max(errors, key=lambda item: item[1])[0]
        del current[worst]
    return None


def _choose_mapping(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    mappings: Sequence[dict[int, int] | None],
    *,
    min_size: int = 3,
) -> dict[int, int] | None:
    """Largest set of atoms with near-zero rigid error.

    A bigger subgraph is kept only when those extra atoms sit on the
    reference too. One imperfect pair is dropped instead of dislodging the
    rest. If no embedding is near zero, the largest subgraph (then lowest
    RMSD) is returned so a template can still redraw it.
    """
    best_clean: dict[int, int] | None = None
    best_clean_key: tuple[int, float] | None = None
    best_raw: dict[int, int] | None = None
    best_raw_key: tuple[int, float] | None = None
    for mapping in mappings:
        if not mapping or len(mapping) < min_size:
            continue
        if len(set(mapping.values())) != len(mapping):
            continue
        raw_key = (-len(mapping), _mapping_rmsd(ref, other, mapping))
        if best_raw_key is None or raw_key < best_raw_key:
            best_raw_key = raw_key
            best_raw = mapping
        clean = _near_zero_core(ref, other, mapping, min_size=min_size)
        if clean is None:
            continue
        clean_key = (-len(clean), _mapping_rmsd(ref, other, clean))
        if best_clean_key is None or clean_key < best_clean_key:
            best_clean_key = clean_key
            best_clean = clean
    if best_clean is not None:
        return best_clean
    return best_raw


_NODE_BUDGET = 8000
_MAP_KEEP = 32


def _bond_ok(
    o: int,
    on: int,
    r: int,
    rn: int,
    oth_adj: dict[int, dict[int, float]],
    ref_adj: dict[int, dict[int, float]],
    ring_o: set[frozenset[int]],
    ring_r: set[frozenset[int]],
) -> bool:
    if rn not in ref_adj[r] or on not in oth_adj[o]:
        return False
    ring = frozenset((o, on)) in ring_o and frozenset((r, rn)) in ring_r
    return _order_ok(oth_adj[o][on], ref_adj[r][rn], ring=ring)


def _maps_from_seed(
    seed_o: int,
    seed_r: int,
    oth_el: dict[int, str],
    ref_el: dict[int, str],
    oth_adj: dict[int, dict[int, float]],
    ref_adj: dict[int, dict[int, float]],
    ring_o: set[frozenset[int]],
    ring_r: set[frozenset[int]],
    *,
    node_budget: int,
) -> tuple[list[dict[int, int]], int]:
    """Maximal connected maps from one seed. Several, not the first branch."""
    found: list[dict[int, int]] = []
    best_size = 0
    nodes = 0

    def compatible(on: int, rn: int, mapping: dict[int, int]) -> bool:
        if oth_el[on] != ref_el[rn]:
            return False
        for o2 in oth_adj[on]:
            if o2 not in mapping:
                continue
            if not _bond_ok(o2, on, mapping[o2], rn, oth_adj, ref_adj, ring_o, ring_r):
                return False
        return True

    def rec(mapping: dict[int, int], inv: dict[int, int]) -> None:
        nonlocal nodes, best_size
        if nodes >= node_budget:
            return
        nodes += 1
        growable: list[tuple[int, int, list[int]]] = []
        for _o, _r in mapping.items():
            for on in oth_adj[_o]:
                if on in mapping:
                    continue
                cands = [rn for rn in ref_adj[_r] if rn not in inv and compatible(on, rn, mapping)]
                if cands:
                    growable.append((len(cands), on, cands))
        if not growable:
            size = len(mapping)
            if size > best_size:
                best_size = size
                found.clear()
                found.append(dict(mapping))
            elif size == best_size and len(found) < _MAP_KEEP:
                found.append(dict(mapping))
            return
        growable.sort(key=lambda item: (item[0], item[1]))
        _n_cands, on, cands = growable[0]
        for rn in sorted(cands):
            mapping[on] = rn
            inv[rn] = on
            rec(mapping, inv)
            del mapping[on]
            del inv[rn]
            if nodes >= node_budget:
                return

    rec({seed_o: seed_r}, {seed_r: seed_o})
    return found, nodes


def _mcs_mapping(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    *,
    min_size: int = 3,
) -> dict[int, int] | None:
    """Largest connected common subgraph, other index → ref index.

    The kept atoms are the largest near-zero rigid fit, not the first seed
    that covers the graph.
    """
    ref_el = {a.index: _element_key(a.element) for a in ref.atoms}
    oth_el = {a.index: _element_key(a.element) for a in other.atoms}
    ref_adj = _adj(ref)
    oth_adj = _adj(other)
    ring_r = _ring_edges(ref_adj)
    ring_o = _ring_edges(oth_adj)

    seeds: list[tuple[int, int]] = []
    for o, oe in oth_el.items():
        for r, re in ref_el.items():
            if oe == re:
                seeds.append((o, r))
    seeds.sort(
        key=lambda p: (
            0 if oth_el[p[0]] != "C" else 1,
            -len(oth_adj[p[0]]),
            -len(ref_adj[p[1]]),
            p[0],
            p[1],
        )
    )
    budget = _NODE_BUDGET
    found: list[dict[int, int]] = []
    # Cap seed trials for portability on larger mols.
    for o, r in seeds[: max(24, len(oth_el) * 2)]:
        if budget <= 0:
            break
        maps, used = _maps_from_seed(
            o,
            r,
            oth_el,
            ref_el,
            oth_adj,
            ref_adj,
            ring_o,
            ring_r,
            node_budget=budget,
        )
        budget -= used
        found.extend(maps)
    return _choose_mapping(ref, other, found, min_size=min_size)


def _kabsch_2d(
    src: list[tuple[float, float]],
    dst: list[tuple[float, float]],
    *,
    allow_reflect: bool = True,
) -> tuple[float, float, float, float, float]:
    """Return ``(cos, sin, tx, ty, det_sign)`` mapping src → dst."""
    n = len(src)
    if n == 0:
        return 1.0, 0.0, 0.0, 0.0, 1.0
    sx = sum(p[0] for p in src) / n
    sy = sum(p[1] for p in src) / n
    dx = sum(p[0] for p in dst) / n
    dy = sum(p[1] for p in dst) / n
    sxx = syy = sxy = syx = 0.0
    for (x, y), (u, v) in zip(src, dst, strict=True):
        x0, y0 = x - sx, y - sy
        u0, v0 = u - dx, v - dy
        sxx += x0 * u0
        sxy += x0 * v0
        syx += y0 * u0
        syy += y0 * v0

    def rot_score(c: float, s: float) -> float:
        # Score of x' = c x - s y, y' = s x + c y. The sine term is
        # (src_x * dst_y - src_y * dst_x), not the opposite.
        return c * (sxx + syy) + s * (sxy - syx)

    ang = math.atan2(sxy - syx, sxx + syy)
    c1, s1 = math.cos(ang), math.sin(ang)
    best_c, best_s, best_det = c1, s1, 1.0
    best_sc = rot_score(c1, s1)
    if allow_reflect:
        ang2 = math.atan2(sxy + syx, sxx - syy)
        c2, s2 = math.cos(ang2), math.sin(ang2)
        sc2 = c2 * (sxx - syy) + s2 * (sxy + syx)
        if sc2 > best_sc:
            best_c, best_s, best_det, best_sc = c2, s2, -1.0, sc2

    if best_det >= 0:
        cos_r, sin_r = best_c, best_s
        tx = dx - (cos_r * sx - sin_r * sy)
        ty = dy - (sin_r * sx + cos_r * sy)
    else:
        cos_r, sin_r = best_c, best_s
        tx = dx - (cos_r * sx + sin_r * sy)
        ty = dy - (sin_r * sx - cos_r * sy)
    return cos_r, sin_r, tx, ty, best_det


def _apply_transform(
    atoms: list[AtomLayout],
    cos_r: float,
    sin_r: float,
    tx: float,
    ty: float,
    det: float,
) -> list[AtomLayout]:
    out: list[AtomLayout] = []
    for a in atoms:
        if det >= 0:
            x = cos_r * a.x - sin_r * a.y + tx
            y = sin_r * a.x + cos_r * a.y + ty
        else:
            x = cos_r * a.x + sin_r * a.y + tx
            y = sin_r * a.x - cos_r * a.y + ty
        out.append(a.model_copy(update={"x": x, "y": y}))
    return out


class Aligner(Protocol):
    """Common alignment interface. Template when possible, else rigid."""

    name: str
    supports_template: bool

    def map_atoms(self, ref: MoleculeLayout, other: MoleculeLayout) -> dict[int, int] | None:
        """Matched atoms as other index → reference index. None if too small."""
        ...

    def rigid_align(
        self,
        ref: MoleculeLayout,
        other: MoleculeLayout,
        mapping: dict[int, int],
    ) -> MoleculeLayout:
        """Rotate/translate ``other`` onto ``ref`` using ``mapping``."""
        ...

    def depict_on_template(
        self,
        ref: MoleculeLayout,
        other: MoleculeLayout,
        mapping: dict[int, int],
        *,
        smiles: str | None = None,
    ) -> MoleculeLayout | None:
        """Redraw ``other`` with mapped atoms fixed to ``ref``. None if unsupported."""
        ...


def _invert_tetrahedral(bond: BondLayout) -> BondLayout:
    """A reflection reverses the 2D order of substituents, so up and down swap."""
    if bond.stereo == "up":
        return bond.model_copy(update={"stereo": "down"})
    if bond.stereo == "down":
        return bond.model_copy(update={"stereo": "up"})
    return bond


def _with_warning(layout: MoleculeLayout, text: str) -> MoleculeLayout:
    if text in layout.warnings:
        return layout
    return layout.model_copy(update={"warnings": [*layout.warnings, text]})


# Public re-exports for align_rdkit / shared helpers.
choose_mapping = _choose_mapping
mcs_mapping = _mcs_mapping
with_warning = _with_warning


class RigidAligner:
    """Depict-then-superimpose. The fallback every backend can do."""

    name = "rigid"
    supports_template = False

    def map_atoms(self, ref: MoleculeLayout, other: MoleculeLayout) -> dict[int, int] | None:
        return _mcs_mapping(ref, other)

    def rigid_align(
        self,
        ref: MoleculeLayout,
        other: MoleculeLayout,
        mapping: dict[int, int],
    ) -> MoleculeLayout:
        by_index_ref = {a.index: a for a in ref.atoms}
        by_index = {a.index: a for a in other.atoms}
        oth_ids = [i for i in mapping if i in by_index and mapping[i] in by_index_ref]
        if len(oth_ids) < 1:
            return other
        src = [(by_index[i].x, by_index[i].y) for i in oth_ids]
        dst = [(by_index_ref[mapping[i]].x, by_index_ref[mapping[i]].y) for i in oth_ids]
        cos_r, sin_r, tx, ty, det = _kabsch_2d(src, dst)
        new_atoms = _apply_transform(other.atoms, cos_r, sin_r, tx, ty, det)
        bonds = other.bonds
        if det < 0:
            bonds = [_invert_tetrahedral(b) for b in other.bonds]
        return other.model_copy(update={"atoms": new_atoms, "bonds": bonds})

    def depict_on_template(
        self,
        ref: MoleculeLayout,
        other: MoleculeLayout,
        mapping: dict[int, int],
        *,
        smiles: str | None = None,
    ) -> MoleculeLayout | None:
        return None


def select_aligner() -> Aligner:
    """RDKit template aligner when the package imports, otherwise rigid."""
    try:
        from xpict.align_rdkit import RdkitAligner, rdkit_available
    except ImportError:
        return RigidAligner()
    if rdkit_available():
        return RdkitAligner()
    return RigidAligner()


def align_to_reference(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    aligner: Aligner,
    *,
    smiles: str | None = None,
) -> MoleculeLayout:
    """Put ``other`` in ``ref``'s frame. Template first, rigid if that cannot."""
    mapping = aligner.map_atoms(ref, other)
    if not mapping:
        return other
    if aligner.supports_template:
        templated = aligner.depict_on_template(ref, other, mapping, smiles=smiles)
        if templated is not None:
            return templated
        snapped = aligner.rigid_align(ref, other, mapping)
        return _with_warning(snapped, "alignment: rigid fallback after template failure")
    return _with_warning(aligner.rigid_align(ref, other, mapping), "alignment: rigid transform")


def align_layouts(
    layouts: Sequence[MoleculeLayout],
    *,
    enabled: bool = False,
    aligner: Aligner | None = None,
    specs: Sequence[MoleculeSpec] | None = None,
) -> list[MoleculeLayout]:
    """Align molecules onto the first layout's frame when ``enabled``."""
    if not enabled or len(layouts) < 2:
        return list(layouts)

    chosen = aligner if aligner is not None else select_aligner()
    smiles: list[str | None] = []
    if specs is not None:
        from xpict.structure import structure_smiles

        smiles = [structure_smiles(spec) for spec in specs]
    ref = layouts[0]
    out: list[MoleculeLayout] = [ref]
    for i, lay in enumerate(layouts[1:], start=1):
        smi = smiles[i] if i < len(smiles) else None
        out.append(align_to_reference(ref, lay, chosen, smiles=smi))
    return out

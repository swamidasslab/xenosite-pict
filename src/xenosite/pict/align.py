"""Multi-molecule chemical alignment (pure Python — no NetworkX).

Aligns subsequent molecules onto the first via a **connected** common-subgraph
grow + rigid 2D transform. Stdlib only (portable for WASM / slim installs).

ELK/grid still place viewports after this shared chemical frame is applied.
"""

from __future__ import annotations

import math
from collections import defaultdict
from collections.abc import Sequence

from xenosite.pict.contracts.layout import AtomLayout, MoleculeLayout


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


def _order_ok(a: float, b: float) -> bool:
    if abs(a - b) < 0.1:
        return True
    return min(a, b) >= 1.4 and max(a, b) <= 2.1


def _grow_connected(
    seed_o: int,
    seed_r: int,
    oth_el: dict[int, str],
    ref_el: dict[int, str],
    oth_adj: dict[int, dict[int, float]],
    ref_adj: dict[int, dict[int, float]],
) -> dict[int, int]:
    """BFS grow a connected mapping from a seed pair (other→ref)."""
    mapping = {seed_o: seed_r}
    inv = {seed_r: seed_o}
    queue = [(seed_o, seed_r)]
    while queue:
        o, r = queue.pop(0)
        # Pair unmapped neighbors by element + bond order.
        o_nbrs = [(n, ord_) for n, ord_ in oth_adj[o].items() if n not in mapping]
        r_nbrs = [(n, ord_) for n, ord_ in ref_adj[r].items() if n not in inv]
        # Greedy: match each other-neighbor to a unique compatible ref-neighbor.
        used_r: set[int] = set()
        for on, oo in sorted(o_nbrs, key=lambda t: (oth_el[t[0]], t[0])):
            match = None
            for rn, ro in sorted(r_nbrs, key=lambda t: (ref_el[t[0]], t[0])):
                if rn in used_r:
                    continue
                if oth_el[on] != ref_el[rn]:
                    continue
                if not _order_ok(oo, ro):
                    continue
                match = rn
                break
            if match is None:
                continue
            used_r.add(match)
            mapping[on] = match
            inv[match] = on
            queue.append((on, match))
    return mapping


def _mcs_mapping(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    *,
    min_size: int = 3,
) -> dict[int, int] | None:
    """Largest connected common subgraph mapping: other index → ref index."""
    ref_el = {a.index: _element_key(a.element) for a in ref.atoms}
    oth_el = {a.index: _element_key(a.element) for a in other.atoms}
    ref_adj = _adj(ref)
    oth_adj = _adj(other)

    best: dict[int, int] = {}
    # Seed on matching element pairs; prefer heteroatoms / high degree.
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
    # Cap seed trials for portability on larger mols.
    for o, r in seeds[: max(24, len(oth_el) * 2)]:
        mapping = _grow_connected(o, r, oth_el, ref_el, oth_adj, ref_adj)
        if len(mapping) > len(best):
            best = mapping
        if len(best) >= min(len(ref_el), len(oth_el)):
            break
    if len(best) < min_size:
        return None
    return best


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
        return c * (sxx + syy) + s * (syx - sxy)

    ang = math.atan2(syx - sxy, sxx + syy)
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


def align_layouts(
    layouts: Sequence[MoleculeLayout],
    *,
    enabled: bool = False,
) -> list[MoleculeLayout]:
    """Align molecules onto the first layout's frame when ``enabled``.

    Pure Python connected-MCS + Kabsch — no NetworkX / chem engines.
    """
    if not enabled or len(layouts) < 2:
        return list(layouts)

    ref = layouts[0]
    by_index_ref = {a.index: a for a in ref.atoms}
    out: list[MoleculeLayout] = [ref]
    for lay in layouts[1:]:
        mapping = _mcs_mapping(ref, lay)
        if not mapping:
            out.append(lay)
            continue
        by_index = {a.index: a for a in lay.atoms}
        oth_ids = list(mapping.keys())
        src = [(by_index[i].x, by_index[i].y) for i in oth_ids]
        dst = [(by_index_ref[mapping[i]].x, by_index_ref[mapping[i]].y) for i in oth_ids]
        cos_r, sin_r, tx, ty, det = _kabsch_2d(src, dst)
        new_atoms = _apply_transform(lay.atoms, cos_r, sin_r, tx, ty, det)
        out.append(lay.model_copy(update={"atoms": new_atoms}))
    return out

"""Align later molecules onto the first molecule's coordinate frame.

Two implementations share one interface (``Aligner``):

* **Template** (RDKit, when installed): matched atoms are fixed on the
  reference coordinates and the rest of the molecule is depicted around
  them. This is the correct 2D alignment.
* **Rigid** (always available): each molecule is depicted on its own, then
  rotated and translated so the matched atoms overlap as well as a rigid
  move allows. Indigo and native cannot do the template step.

``align_layouts`` picks RDKit when it imports, and falls back to rigid if
template depiction fails or RDKit is absent. This is not a layout backend
and does not replace Indigo or native coordinate generation for the
reference molecule.
"""

from __future__ import annotations

import math
from collections import defaultdict
from collections.abc import Sequence
from typing import Protocol

from xenosite.pict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xenosite.pict.contracts.spec import MoleculeSpec


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

    def map_atoms(
        self, ref: MoleculeLayout, other: MoleculeLayout
    ) -> dict[int, int] | None:
        """Matched atoms as other index → reference index. None if too small."""

    def rigid_align(
        self,
        ref: MoleculeLayout,
        other: MoleculeLayout,
        mapping: dict[int, int],
    ) -> MoleculeLayout:
        """Rotate/translate ``other`` onto ``ref`` using ``mapping``."""

    def depict_on_template(
        self,
        ref: MoleculeLayout,
        other: MoleculeLayout,
        mapping: dict[int, int],
        *,
        smiles: str | None = None,
    ) -> MoleculeLayout | None:
        """Redraw ``other`` with mapped atoms fixed to ``ref``. None if unsupported."""


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


class RigidAligner:
    """Depict-then-superimpose. The fallback every backend can do."""

    name = "rigid"
    supports_template = False

    def map_atoms(
        self, ref: MoleculeLayout, other: MoleculeLayout
    ) -> dict[int, int] | None:
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
        from xenosite.pict.align_rdkit import RdkitAligner, rdkit_available
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
        from xenosite.pict.structure import structure_smiles

        smiles = [structure_smiles(spec) for spec in specs]
    ref = layouts[0]
    out: list[MoleculeLayout] = [ref]
    for i, lay in enumerate(layouts[1:], start=1):
        smi = smiles[i] if i < len(smiles) else None
        out.append(align_to_reference(ref, lay, chosen, smiles=smi))
    return out

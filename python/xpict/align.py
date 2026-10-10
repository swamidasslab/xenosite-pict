"""Align later molecules onto the first molecule's coordinate frame.

Mapping + soft-constrained CoordGen live in Rust (``xpict._native``).
Python converts ``MoleculeLayout`` ↔ flat arrays and applies results.
There is no rigid/Kabsch fallback — if CoordGen cannot align, the layout
is left unchanged.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

from xpict import _native as _rust
from xpict.contracts.layout import AtomLayout, MoleculeLayout

_COORDGEN_BOND = 50.0


def _atomic_num(el: str) -> int:
    if el == "*" or el.startswith("R") or el.startswith("_"):
        return 0
    n = _rust.atomic_number(el)
    return int(n) if n is not None else 0


def _layout_to_flat(layout: MoleculeLayout) -> dict:
    atoms = sorted(layout.atoms, key=lambda a: a.index)
    index_of = {a.index: i for i, a in enumerate(atoms)}
    atomic_numbers = [_atomic_num(a.element) for a in atoms]
    formal_charges = [int(a.charge) for a in atoms]
    bond_from: list[int] = []
    bond_to: list[int] = []
    bond_orders: list[int] = []
    for b in layout.bonds:
        if b.begin not in index_of or b.end not in index_of:
            continue
        bond_from.append(index_of[b.begin])
        bond_to.append(index_of[b.end])
        bond_orders.append(max(1, min(3, int(round(b.order or 1.0)))))
    return {
        "atomic_numbers": atomic_numbers,
        "formal_charges": formal_charges,
        "bond_from": bond_from,
        "bond_to": bond_to,
        "bond_orders": bond_orders,
        "bond_stereos": [0] * len(bond_from),
        "atoms": atoms,
        "x": [float(a.x) for a in atoms],
        "y": [float(a.y) for a in atoms],
    }


def _mean_bond_length(layout: MoleculeLayout) -> float:
    by = {a.index: a for a in layout.atoms}
    lens = [
        math.hypot(by[b.begin].x - by[b.end].x, by[b.begin].y - by[b.end].y)
        for b in layout.bonds
        if b.begin in by and b.end in by
    ]
    return sum(lens) / len(lens) if lens else 1.0


def cost_map(ref: MoleculeLayout, other: MoleculeLayout) -> dict[int, int] | None:
    """Query→ref atom index map from Rust cost MCS, or ``None`` if unavailable."""
    if not getattr(_rust, "HAS_COORDGEN", False):
        return None
    rf = _layout_to_flat(ref)
    of = _layout_to_flat(other)
    pairs, _cost = _rust.cost_mapping(
        rf["atomic_numbers"],
        rf["bond_from"],
        rf["bond_to"],
        rf["bond_orders"],
        of["atomic_numbers"],
        of["bond_from"],
        of["bond_to"],
        of["bond_orders"],
        ref_formal_charges=rf["formal_charges"],
        ref_bond_stereos=rf["bond_stereos"],
        query_formal_charges=of["formal_charges"],
        query_bond_stereos=of["bond_stereos"],
    )
    if not pairs:
        return None
    ref_atoms = rf["atoms"]
    oth_atoms = of["atoms"]
    return {int(oth_atoms[qi].index): int(ref_atoms[ri].index) for qi, ri in pairs}


# Historical aliases used by tests / older call sites.
mcs_mapping = cost_map
_mcs_mapping = cost_map


def _apply_xy(
    layout: MoleculeLayout,
    x: list[float],
    y: list[float],
    *,
    dense_atoms: list[AtomLayout],
) -> MoleculeLayout:
    by_dense = {a.index: (x[i], y[i]) for i, a in enumerate(dense_atoms)}
    new_atoms = [
        a.model_copy(update={"x": by_dense[a.index][0], "y": by_dense[a.index][1]})
        if a.index in by_dense
        else a
        for a in layout.atoms
    ]
    return layout.model_copy(update={"atoms": new_atoms})


def _with_warning(layout: MoleculeLayout, text: str) -> MoleculeLayout:
    if text in layout.warnings:
        return layout
    return layout.model_copy(update={"warnings": [*layout.warnings, text]})


def align_to_reference(
    ref: MoleculeLayout,
    other: MoleculeLayout,
    *,
    smiles: str | None = None,
) -> MoleculeLayout:
    """Soft-constrained CoordGen align of ``other`` onto ``ref``.

    Returns ``other`` unchanged when CoordGen is unavailable or mapping/layout fails.
    """
    _ = smiles
    if not getattr(_rust, "HAS_COORDGEN", False):
        return other
    rf = _layout_to_flat(ref)
    of = _layout_to_flat(other)
    to_cg = _COORDGEN_BOND / max(_mean_bond_length(ref), 1e-6)
    try:
        pairs, _cost, _rx, _ry, qx, qy = _rust.align_molecule_constrained(
            rf["atomic_numbers"],
            rf["bond_from"],
            rf["bond_to"],
            rf["bond_orders"],
            of["atomic_numbers"],
            of["bond_from"],
            of["bond_to"],
            of["bond_orders"],
            ref_formal_charges=rf["formal_charges"],
            ref_bond_stereos=rf["bond_stereos"],
            ref_x=[x * to_cg for x in rf["x"]],
            ref_y=[y * to_cg for y in rf["y"]],
            query_formal_charges=of["formal_charges"],
            query_bond_stereos=of["bond_stereos"],
        )
    except Exception:
        return other
    if not pairs:
        return other
    inv = 1.0 / to_cg
    laid = _apply_xy(
        other,
        [float(v) * inv for v in qx],
        [float(v) * inv for v in qy],
        dense_atoms=of["atoms"],
    )
    return _with_warning(laid, "alignment: coordgen constrained")


def align_layouts(
    layouts: Sequence[MoleculeLayout],
    *,
    enabled: bool = False,
) -> list[MoleculeLayout]:
    """Align molecules onto the first layout's frame when ``enabled``."""
    if not enabled or len(layouts) < 2:
        return list(layouts)
    ref = layouts[0]
    return [ref, *[align_to_reference(ref, lay) for lay in layouts[1:]]]

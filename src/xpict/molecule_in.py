"""Convert MoleculeLayout / MoleculeSpec → Rust ``MoleculeIn`` JSON dict."""

from __future__ import annotations

from typing import Any

from xpict.contracts.layout import MoleculeLayout
from xpict.contracts.spec import MoleculeSpec, ShadeSpec
from xpict.draw.metrics import BOND_PX, coord_scale


def layout_to_molecule_in(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec | None = None,
    *,
    scale: float | None = None,
) -> dict[str, Any]:
    """SVG-space MoleculeIn for ``_native.depict_molecule``.

    Matches JS scaling: mean bond → ``BOND_PX``, Y flipped into SVG space.
    """
    from xpict.draw.drawable import display_text
    from xpict.draw.markush import apply_rgroup_texts

    if scale is None:
        scale = coord_scale(layout)
    ys = [a.y for a in layout.atoms]
    max_y = max(ys) if ys else 0.0

    labels = [display_text(a) for a in layout.atoms]
    if mol_spec is not None:
        labels = apply_rgroup_texts(layout, mol_spec, labels)

    atoms: list[dict[str, Any]] = []
    for a, label in zip(layout.atoms, labels, strict=True):
        x = a.x * scale
        y = (max_y - a.y) * scale
        entry: dict[str, Any] = {
            "index": a.index,
            "element": a.element,
            "x": x,
            "y": y,
        }
        if a.charge:
            entry["charge"] = a.charge
        if label:
            entry["label"] = label
        atoms.append(entry)

    bonds: list[dict[str, Any]] = []
    for b in layout.bonds:
        bd: dict[str, Any] = {
            "index": b.index,
            "begin": b.begin,
            "end": b.end,
            "order": b.order,
        }
        if b.stereo and b.stereo != "none":
            bd["stereo"] = b.stereo
        bonds.append(bd)

    out: dict[str, Any] = {
        "atoms": atoms,
        "bonds": bonds,
    }
    mid = layout.id or (mol_spec.id if mol_spec else None)
    if mid:
        out["id"] = mid
    if mol_spec and mol_spec.color:
        out["color"] = mol_spec.color
    if mol_spec and mol_spec.shade is not None:
        atom_s, bond_s = _shade_vectors(layout, mol_spec.shade)
        if atom_s is not None:
            out["atom_shade"] = atom_s
        if bond_s is not None:
            out["bond_shade"] = bond_s
    if mol_spec and mol_spec.marks:
        mark_atoms: list[int] = []
        mark_bonds: list[list[int]] = []
        for mark in mol_spec.marks:
            if mark.atoms:
                mark_atoms.extend(mark.atoms)
            if mark.bonds:
                for a, b in mark.bonds:
                    mark_bonds.append([a, b])
        if mark_atoms:
            out["mark_atoms"] = mark_atoms
        if mark_bonds:
            out["mark_bonds"] = mark_bonds
    return out


def _shade_vectors(
    layout: MoleculeLayout, shade: ShadeSpec
) -> tuple[list[float] | None, list[float] | None]:
    atom_s = None
    bond_s = None
    if shade.atoms is not None:
        atom_s = [float(v) for v in shade.atoms]
        # pad / trim to atom count
        n = len(layout.atoms)
        if len(atom_s) < n:
            atom_s = atom_s + [0.0] * (n - len(atom_s))
        else:
            atom_s = atom_s[:n]
    if shade.bonds is not None:
        bond_s = [float(v) for v in shade.bonds]
        n = len(layout.bonds)
        if len(bond_s) < n:
            bond_s = bond_s + [0.0] * (n - len(bond_s))
        else:
            bond_s = bond_s[:n]
    return atom_s, bond_s

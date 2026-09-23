"""Markush helpers: rgroups on stars, named rings, ring attachments."""

from __future__ import annotations

from xpict.contracts.layout import MoleculeLayout
from xpict.future.spec import (
    AnnotationSpec,
    AnnotKind,
    MoleculeSpec,
)


def star_atom_indices(layout: MoleculeLayout) -> list[int]:
    """Atom indices of ``*`` in layout order (star ordinal 0, 1, …)."""
    return [a.index for a in layout.atoms if a.element == "*"]


def resolve_rgroups(
    layout: MoleculeLayout,
    rgroups: list[str | None] | dict[str, str | None] | None,
) -> dict[int, str | None]:
    """Map atom index → label for stars.

    ``None`` values mean an explicit bare ``*`` (suppress CX alias if any).
    Missing ordinals keep the layout's existing label / default ``*``.
    """
    stars = star_atom_indices(layout)
    if not rgroups or not stars:
        return {}
    out: dict[int, str | None] = {}
    if isinstance(rgroups, list):
        for i, label in enumerate(rgroups):
            if i >= len(stars):
                break
            out[stars[i]] = label
        return out
    for key, label in rgroups.items():
        try:
            ordinal = int(key)
        except (TypeError, ValueError):
            continue
        if 0 <= ordinal < len(stars):
            out[stars[ordinal]] = label
    return out


def resolve_ring_atoms(
    ring: list[int] | str,
    rings: dict[str, list[int]],
) -> list[int]:
    """Resolve a ring name or atom list to atom indices."""
    if isinstance(ring, str):
        atoms = rings.get(ring)
        if atoms is None:
            raise KeyError(f"unknown ring {ring!r}")
        return list(atoms)
    return list(ring)


def ring_attachment_annotations(
    mol_spec: MoleculeSpec,
) -> list[AnnotationSpec]:
    """Expand :attr:`MoleculeSpec.ring_attachments` into callout annotations."""
    out: list[AnnotationSpec] = []
    color = mol_spec.color
    for ra in mol_spec.ring_attachments:
        atoms = resolve_ring_atoms(ra.ring, mol_spec.rings)
        if not atoms:
            continue
        out.append(
            AnnotationSpec(
                kind=AnnotKind.callout,
                ring=atoms,
                label=ra.label,
                color=color,
                arrow=True,
                prefer=ra.prefer,
            )
        )
    return out


def apply_rgroup_texts(
    layout: MoleculeLayout,
    mol_spec: MoleculeSpec,
    texts: list[str | None],
) -> list[str | None]:
    """Return a copy of ``texts`` with star labels applied.

    ``star_labels`` (public document / single-mol parity) wins over future
    ``rgroups`` when both are set.
    """
    labels: list[str | None] | dict[str, str | None] | None
    if mol_spec.star_labels is not None:
        labels = mol_spec.star_labels
    else:
        labels = mol_spec.rgroups
    overrides = resolve_rgroups(layout, labels)
    if not overrides:
        return list(texts)
    by_index = {a.index: i for i, a in enumerate(layout.atoms)}
    out = list(texts)
    for atom_index, label in overrides.items():
        slot = by_index.get(atom_index)
        if slot is None:
            continue
        if label is None or label == "":
            out[slot] = "*"
        else:
            out[slot] = label
    return out


def rtable_groups(mol_spec: MoleculeSpec) -> list[str]:
    """Column headers for an R table (explicit or first-seen labels)."""
    if mol_spec.rtable is not None and mol_spec.rtable.groups:
        return list(mol_spec.rtable.groups)
    seen: list[str] = []
    if isinstance(mol_spec.rgroups, list):
        for lab in mol_spec.rgroups:
            if lab and lab not in seen:
                seen.append(lab)
    elif isinstance(mol_spec.rgroups, dict):
        # Preserve ordinal order.
        for key in sorted(mol_spec.rgroups, key=lambda k: int(k) if str(k).isdigit() else k):
            lab = mol_spec.rgroups[key]
            if lab and lab not in seen:
                seen.append(lab)
    for ra in mol_spec.ring_attachments:
        if ra.label and ra.label not in seen:
            seen.append(ra.label)
    return seen


__all__ = [
    "apply_rgroup_texts",
    "resolve_rgroups",
    "resolve_ring_atoms",
    "ring_attachment_annotations",
    "rtable_groups",
    "star_atom_indices",
]

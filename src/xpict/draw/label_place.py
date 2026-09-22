"""Atom-label orientation + backbone insets (Python wrapper over Rust labels).

**Keep in Python for now** — do not fold this into the JS-mirrored
``depict_molecule``-only path yet. Callers that need orientation / backbone
insets outside Rust paint still import from here.

Coords are caller SVG positions. Labels split into an atom-center part and an
optional traveling part (`OH` → center ``O``, travel ``H``). On the west side
of a molecule the display string flips to ``HO`` while ``O`` stays on the atom.
Abbreviations without a traveling part (``GlcA``) keep their spelling; the
first or last glyph is the center depending on the bond approach side.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal, Sequence

from xpict import _native
from xpict.contracts.layout import BondLayout
from xpict.draw.metrics import FONT_PX

LabelSide = Literal["east", "west", "north", "south"]


@dataclass(frozen=True, slots=True)
class PlacedLabel:
    text: str
    origin_x: float
    y: float
    atom_x: float
    atom_y: float
    side: LabelSide
    clearance: float
    path_d: str = ""
    #: Original label string before orientation (for re-outline / halo).
    raw: str = ""


def split_atom_label(raw: str) -> tuple[str, str]:
    """Return ``(center, traveling)``."""
    return tuple(_native.split_atom_label(raw))  # type: ignore[return-value]


def compose_atom_label(center: str, traveling: str, side: LabelSide) -> str:
    return str(_native.compose_atom_label(center, traveling, side))


def place_backbone(
    coords: Sequence[tuple[float, float]],
    texts: Sequence[str | None],
    bonds: Sequence[BondLayout] | Sequence[tuple[int, int]],
    *,
    font_size: float = FONT_PX,
    atom_indices: Sequence[int] | None = None,
) -> tuple[list[tuple[float, float, float, float]], list[PlacedLabel | None]]:
    """Place labels and shorten bonds into center-glyph clearances.

    ``coords[i]`` / ``texts[i]`` are parallel. Bond endpoints are atom
    **indices** (layout atom index) when ``atom_indices`` maps slot→index;
    otherwise bond endpoints are treated as positions into ``coords``.
    """
    atoms = [(float(x), float(y), texts[i]) for i, (x, y) in enumerate(coords)]
    index_of: dict[int, int] | None = None
    if atom_indices is not None:
        index_of = {idx: i for i, idx in enumerate(atom_indices)}

    bond_pairs: list[tuple[int, int]] = []
    raw_bonds: list[tuple[int, int]] = []
    for b in bonds:
        if isinstance(b, tuple):
            a, c = int(b[0]), int(b[1])
        else:
            a, c = int(b.begin), int(b.end)
        raw_bonds.append((a, c))
        if index_of is not None:
            if a not in index_of or c not in index_of:
                bond_pairs.append((0, 0))  # placeholder; fixed below
            else:
                bond_pairs.append((index_of[a], index_of[c]))
        else:
            bond_pairs.append((a, c))

    ends, raw_labels = _native.place_backbone(atoms, bond_pairs, font_size)
    # Restore unmapped bonds to full atom–atom segments.
    fixed_ends: list[tuple[float, float, float, float]] = []
    for (a, c), end in zip(raw_bonds, ends, strict=True):
        if index_of is not None and (a not in index_of or c not in index_of):
            # Should not happen for normal layouts; keep a degenerate segment.
            fixed_ends.append((0.0, 0.0, 0.0, 0.0))
            continue
        ia = index_of[a] if index_of is not None else a
        ic = index_of[c] if index_of is not None else c
        if ia >= len(coords) or ic >= len(coords):
            fixed_ends.append((0.0, 0.0, 0.0, 0.0))
            continue
        fixed_ends.append(
            (float(end[0]), float(end[1]), float(end[2]), float(end[3]))
        )
    labels: list[PlacedLabel | None] = []
    for item in raw_labels:
        if item is None:
            labels.append(None)
            continue
        text, ox, y, ax, ay, side, clearance, path_d, raw = item
        labels.append(
            PlacedLabel(
                text=text,
                origin_x=float(ox),
                y=float(y),
                atom_x=float(ax),
                atom_y=float(ay),
                side=side if side in ("east", "west", "north", "south") else "east",
                clearance=float(clearance),
                path_d=str(path_d or ""),
                raw=str(raw or text),
            )
        )
    return fixed_ends, labels


__all__ = [
    "LabelSide",
    "PlacedLabel",
    "compose_atom_label",
    "place_backbone",
    "split_atom_label",
]

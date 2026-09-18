"""Depiction proportions.

House style is **xenopict** (``drawer.py``): RDKit draws the molecule, then
stroke width is forced to ``scale * 0.1`` with round caps, bond length
``scale = 20``, halo lines ``scale * 0.2``, shade dots ``scale * 0.9``,
atom-mark radius ``scale``, font left at RDKit ``baseFontSize = 0.6``.

Geometry that xenopict does not override comes from the engines it sits on:

- double/triple offset ``0.15 × bond`` (RDKit ``multipleBondOffset``;
  Indigo SVG measured ~0.167)
- wedge fat-end ~``0.30 × bond`` so the triangle still reads against a
  stroke that is already ``0.10 × bond`` (Indigo’s thin-stroke wedges
  were ~0.17 and disappear under xenopict-weight lines)

``coord_scale`` maps layout units onto ``BOND_PX`` so Indigo (bond ≈ 1)
and native (bond = 1.5) depict at the same size.
"""

from __future__ import annotations

import math

from xenosite.pict.contracts.layout import MoleculeLayout

# xenopict ``Xenopict.scale`` — RDKit ``fixedBondLength``.
BOND_PX = 20.0

STROKE_FRAC = 0.10  # xenopict: stroke-width = scale * 0.1
OFFSET_FRAC = 0.15  # RDKit multipleBondOffset (xenopict keeps this)
WEDGE_WIDTH_FRAC = 0.30  # full width at fat end; > stroke so wedges read
FONT_FRAC = 0.60  # RDKit baseFontSize, inherited by xenopict
# xenopict ``reframe`` padding is 1.5× scale around atom centers. Ours
# already includes label overflow in the box, so 1.0× is the same air.
PAD_FRAC = 1.0
END_GAP_FRAC = 0.13
HASH_PER_BOND = 8
HALO_FRAC = 0.20  # xenopict mol_halo lines: scale * 0.2
SHADE_FRAC = 0.90  # xenopict shade(): scale * 0.9
MARK_FRAC = 1.0  # xenopict mark_atoms radius = scale * mark_down_scale

STROKE_PX = STROKE_FRAC * BOND_PX
OFFSET_PX = OFFSET_FRAC * BOND_PX
WEDGE_HALF_PX = WEDGE_WIDTH_FRAC * BOND_PX * 0.5
FONT_PX = FONT_FRAC * BOND_PX
PAD_PX = PAD_FRAC * BOND_PX
END_GAP_PX = END_GAP_FRAC * BOND_PX
HALO_STROKE = HALO_FRAC * BOND_PX
RADICAL_DOT_R = 0.08 * BOND_PX
RADICAL_BASE = 0.55 * BOND_PX
RADICAL_BASE_BARE = 0.35 * BOND_PX


def coord_scale(layout: MoleculeLayout) -> float:
    """SVG pixels per layout unit so the mean bond draws at ``BOND_PX``."""
    by_index = {a.index: a for a in layout.atoms}
    lengths: list[float] = []
    for bond in layout.bonds:
        a = by_index.get(bond.begin)
        b = by_index.get(bond.end)
        if a is None or b is None:
            continue
        lengths.append(math.hypot(a.x - b.x, a.y - b.y))
    mean = sum(lengths) / len(lengths) if lengths else 1.0
    if mean < 1e-6:
        mean = 1.0
    return BOND_PX / mean


def label_clearance(text: str, font_px: float = FONT_PX) -> float:
    """Bond inset so the stroke stops before a centered label.

    Helvetica caps are ~0.62 em; half-width plus a small pad. Xenopict
    leaves this to RDKit’s label padding.
    """
    n = max(len(text), 1)
    # Caps advance ~0.62 em (Helvetica). Half-width plus pad so the stroke
    # stops at the glyph box, not inside it.
    half = 0.62 * font_px * n * 0.5
    pad = 0.22 * font_px
    return half + pad


def hash_count(length: float) -> int:
    """Dash count scaling with drawn bond length."""
    n = round(HASH_PER_BOND * length / BOND_PX)
    return max(5, min(12, n))

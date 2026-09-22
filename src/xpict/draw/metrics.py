"""Depiction proportions.

House style starts from **xenopict** (``drawer.py``): bond length
``scale = 20``, shade dots ``scale * 0.9``, atom-mark radius ``scale``,
font at RDKit ``baseFontSize = 0.6``. Xenopict then forces stroke width
to ``scale * 0.1`` (2 px) and halo lines to ``scale * 0.2``. That stroke
is almost twice the stem of the label face, so the default ink is the
font stem instead, and the halo stays twice that ink.

Geometry that xenopict does not override comes from the engines it sits on:

- double/triple offset ``0.15 × bond`` (RDKit ``multipleBondOffset``;
  Indigo SVG measured ~0.167)
- wedge fat-end ~``0.30 × bond`` so the triangle still reads against the
  bond stroke (Indigo’s thin-stroke wedges were ~0.17)

``coord_scale`` maps layout units onto ``BOND_PX`` so Indigo (bond ≈ 1)
and native (bond = 1.5) depict at the same size.
"""

from __future__ import annotations

import math

from xpict.contracts.layout import MoleculeLayout

# xenopict ``Xenopict.scale`` — RDKit ``fixedBondLength``.
BOND_PX = 20.0

FONT_FRAC = 0.60  # RDKit baseFontSize, inherited by xenopict
# "Helvetica, Arial, sans-serif" resolves to Liberation Sans Regular.
# Outline measurement of the vertical stem (H, I, and the straight stems
# of P/F/B) is 0.0933 em. At FONT_PX that is 1.12 px; xenopict's 2 px
# stroke was nearly twice the letters.
FONT_STEM_EM = 0.0933
STROKE_FRAC = round(FONT_STEM_EM * FONT_FRAC, 3)  # 0.056 → 1.12 px
OFFSET_FRAC = 0.15  # RDKit multipleBondOffset (xenopict keeps this)
# Parallel spacing uses OFFSET_PX even after label insets shorten the stroke;
# see ``bonds.multi_bond_offset``.
WEDGE_WIDTH_FRAC = 0.30  # full width at fat end; > stroke so wedges read
# xenopict ``reframe`` padding is 1.5× scale around atom centers. Ours
# already includes label overflow in the box, so 1.0× is the same air.
PAD_FRAC = 1.0
END_GAP_FRAC = 0.13  # ring doubles: keep the offset off adjacent bonds
# Crossed (either) doubles still use this. Centered acyclic multiples are
# trimmed where they meet neighboring singles, not by a fixed end gap.
CHAIN_END_GAP_FRAC = 0.0
HASH_PER_BOND = 8
HALO_FRAC = 2 * STROKE_FRAC  # knockout stays twice the ink, as in xenopict
# Air between label ink and bond ends, and the Rust buffer on the glyph
# halo. Tuned against RDKit MolDraw2D (~0.9–1.0 px at bond≈20) and Indigo
# terminal-hetero depictions — enough for a dark-host knockout, not a wide
# moat. Molecule captions reuse the same order of air above the title band.
LABEL_GAP_FRAC = 0.05  # × bond → 1.0 px at BOND_PX=20
# Molecule caption (MoleculeSpec.label): default center-bottom band.
TITLE_FONT_FRAC = 0.50  # slightly smaller than atom labels (0.60)
TITLE_BOTTOM_FRAC = 0.20  # typo-box bottom → viewport bottom
TITLE_CLEARANCE_FRAC = 0.08  # mol ink → title typo top (a touch more than LABEL_GAP)
COLLISION_CELL_PX = 4.0  # coarse occupancy stamp for title packing
SHADE_FRAC = 0.90  # xenopict shade(): scale * 0.9
MARK_FRAC = 1.0  # xenopict mark_atoms radius = scale * mark_down_scale
# Annotation callouts / region outlines (box, oval, spline).
ANNOT_PAD_FRAC = 0.45  # region outline stand-off from atom centers
ANNOT_GAP_FRAC = 0.40  # callout label air past the target
ANNOT_FONT_FRAC = 0.50  # same order as molecule captions
ANNOT_ARROW_FRAC = 0.30  # filled arrowhead length
ANNOT_STROKE_FRAC = STROKE_FRAC * 1.25

STROKE_PX = STROKE_FRAC * BOND_PX
OFFSET_PX = OFFSET_FRAC * BOND_PX
WEDGE_HALF_PX = WEDGE_WIDTH_FRAC * BOND_PX * 0.5
FONT_PX = FONT_FRAC * BOND_PX
PAD_PX = PAD_FRAC * BOND_PX
END_GAP_PX = END_GAP_FRAC * BOND_PX
CHAIN_END_GAP_PX = CHAIN_END_GAP_FRAC * BOND_PX
HALO_STROKE = HALO_FRAC * BOND_PX
LABEL_GAP_PX = LABEL_GAP_FRAC * BOND_PX
TITLE_FONT_PX = TITLE_FONT_FRAC * BOND_PX
TITLE_BOTTOM_PX = TITLE_BOTTOM_FRAC * BOND_PX
TITLE_CLEARANCE_PX = TITLE_CLEARANCE_FRAC * BOND_PX
RADICAL_DOT_R = 0.08 * BOND_PX
RADICAL_BASE = 0.55 * BOND_PX
RADICAL_BASE_BARE = 0.35 * BOND_PX
ANNOT_PAD_PX = ANNOT_PAD_FRAC * BOND_PX
ANNOT_GAP_PX = ANNOT_GAP_FRAC * BOND_PX
ANNOT_FONT_PX = ANNOT_FONT_FRAC * BOND_PX
ANNOT_ARROW_PX = ANNOT_ARROW_FRAC * BOND_PX
ANNOT_STROKE_PX = ANNOT_STROKE_FRAC * BOND_PX


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

    Uses the bundled face's real advance and ink width, plus ``LABEL_GAP_PX``
    so the line ends outside the white glyph halo.
    """
    from xpict.draw.text_metrics import measure_text

    return measure_text(text, font_px).clearance()


def hash_count(length: float) -> int:
    """Dash count scaling with drawn bond length."""
    n = round(HASH_PER_BOND * length / BOND_PX)
    return max(5, min(12, n))

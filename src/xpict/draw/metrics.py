"""Depiction proportions.

House style starts from **xenopict** (``drawer.py``): bond length
``scale = 20``, shade dots ``scale * 0.9``, atom-mark radius ``scale``.
RDKit documents ``baseFontSize = 0.6``, but MolDraw2D's outlined heteroatom
glyphs measure closer to **~0.29 × bond** in cap height (~``0.45 × bond``
em with Liberation Sans). We match that optical size. Bond stroke tracks
the label stem (``FONT_STEM_EM × FONT_FRAC``), not xenopict's thicker
``0.10 × bond``.

Geometry that xenopict does not override comes from the engines it sits on:

- double/triple offset ``0.15 × bond`` (RDKit ``multipleBondOffset``;
  Indigo SVG measured ~0.167)
- wedge fat-end ~``0.30 × bond`` so the triangle still reads against the
  bond stroke (Indigo’s thin-stroke wedges were ~0.17)

``coord_scale`` maps layout units onto ``SCALE`` so Indigo (bond ≈ 1)
and native (bond = 1.5) depict at the same size. SVG roots use those
drawing units as ``width``/``height`` (same as viewBox), matching
xenosite.org’s data-URI ``<img>`` intrinsic-size pattern.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

from xpict.contracts.layout import MoleculeLayout

# xenopict ``Xenopict.scale`` / xenosite depiction ``"scale": 20``.
# Layout coords are multiplied so the mean bond draws at this length; the
# SVG root width/height use the same units (intrinsic CSS px when embedded).
SCALE = 20.0
BOND_PX = SCALE  # alias — bond length in drawing / CSS-px units

# Optical match to RDKit MolDraw2D heteroatom labels at fixedBondLength=20
# (measured O cap-height / mean bond ≈ 0.29 → em ≈ 0.45 with Liberation).
FONT_FRAC = 0.45
# "Helvetica, Arial, sans-serif" resolves to Liberation Sans Regular.
# Outline measurement of the vertical stem (H, I, and the straight stems
# of P/F/B) is 0.0933 em. Stroke tracks the stem so ink matches letters.
FONT_STEM_EM = 0.0933
STROKE_FRAC = round(FONT_STEM_EM * FONT_FRAC, 3)  # 0.042 → 0.84 px
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
# halo. Tuned against RDKit MolDraw2D and Indigo terminal-hetero depictions.
LABEL_GAP_FRAC = 0.05  # × bond → 1.0 px at BOND_PX=20
# Molecule caption (MoleculeSpec.label): default center-bottom band.
TITLE_FONT_FRAC = 0.40  # slightly smaller than atom labels
TITLE_BOTTOM_FRAC = 0.20  # typo-box bottom → viewport bottom
TITLE_CLEARANCE_FRAC = 0.08  # mol ink → title typo top (a touch more than LABEL_GAP)
COLLISION_CELL_PX = 4.0  # coarse occupancy stamp for title packing
SHADE_FRAC = 0.90  # xenopict shade(): scale * 0.9
MARK_FRAC = 1.0  # xenopict mark_atoms radius = scale * mark_down_scale
# xenopict mark layer style: stroke-width scale*0.1, opacity 0.7;
# mark halo (<use>): stroke #555, scale*0.2, opacity 0.45.
MARK_STROKE_FRAC = 0.1
MARK_HALO_STROKE_FRAC = 0.2
MARK_OPACITY = 0.7
MARK_HALO_OPACITY = 0.45
MARK_HALO_COLOR = "#555"
# Annotation callouts / region outlines (box, oval, spline).
ANNOT_PAD_FRAC = 0.45  # region outline stand-off from atom centers
ANNOT_GAP_FRAC = 0.40  # callout label air past the target
ANNOT_FONT_FRAC = 0.40  # same order as molecule captions
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
# Halo outline grow (half the label-gap air) + soft white fill.
HALO_GAP_PX = 0.5 * LABEL_GAP_PX
HALO_OPACITY = 0.5
MARK_STROKE_PX = MARK_STROKE_FRAC * BOND_PX
MARK_HALO_STROKE_PX = MARK_HALO_STROKE_FRAC * BOND_PX
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
    """SVG drawing units per layout unit so the mean bond draws at ``SCALE``."""
    return shared_coord_scale([layout])


def shared_coord_scale(layouts: Sequence[MoleculeLayout]) -> float:
    """One scale for co-displayed molecules so mean bonds match ``SCALE``.

    Pools every bond length across ``layouts`` rather than normalizing each
    molecule on its own mean (which made neighbors look differently sized).
    """
    lengths: list[float] = []
    for layout in layouts:
        by_index = {a.index: a for a in layout.atoms}
        for bond in layout.bonds:
            a = by_index.get(bond.begin)
            b = by_index.get(bond.end)
            if a is None or b is None:
                continue
            lengths.append(math.hypot(a.x - b.x, a.y - b.y))
    mean = sum(lengths) / len(lengths) if lengths else 1.0
    if mean < 1e-6:
        mean = 1.0
    return SCALE / mean


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

"""Molecule caption (title) placement: center-bottom with occupancy snug-fit."""

from __future__ import annotations

from dataclasses import dataclass

from xenosite.pict.draw.collision import CollisionGrid
from xenosite.pict.draw.metrics import (
    PAD_PX,
    TITLE_BOTTOM_PX,
    TITLE_CLEARANCE_PX,
    TITLE_FONT_PX,
)
from xenosite.pict.draw.text_metrics import measure_text, text_box


@dataclass(frozen=True)
class TitlePack:
    """Viewport size and vertical shift after packing a bottom title."""

    width: float
    height: float
    dy: float
    title_x: float
    title_y: float  # baseline
    font_size: float
    text: str


def stamp_mol_occupancy(
    grid: CollisionGrid,
    *,
    bond_segments: list[tuple[float, float, float, float, float]],
    label_boxes: list[tuple[float, float, float, float]],
    dots: list[tuple[float, float, float]],
) -> None:
    """Stamp bond capsules, atom-label ink boxes, and radical dots."""
    for x1, y1, x2, y2, radius in bond_segments:
        grid.mark_segment(x1, y1, x2, y2, radius=radius)
    for xmin, ymin, xmax, ymax in label_boxes:
        grid.mark_box(xmin, ymin, xmax, ymax)
    for cx, cy, r in dots:
        grid.mark_circle(cx, cy, r)


def pack_bottom_title(
    *,
    frame_width: float,
    frame_height: float,
    occupancy: CollisionGrid,
    title: str,
    font_size: float = TITLE_FONT_PX,
) -> TitlePack:
    """Place ``title`` center-bottom and snug the molecule down toward it.

    The typographic box sits a fixed ``TITLE_BOTTOM_PX`` above the viewport
    bottom. The molecule is shifted down (``dy``) until its occupancy clears
    the title by ``TITLE_CLEARANCE_PX``, then unused bottom pad is dropped so
    the frame fits snugly. A coarse collision grid drives the clearance — not
    a single bounding box.
    """
    text = title.strip()
    if not text:
        return TitlePack(
            width=frame_width,
            height=frame_height,
            dy=0.0,
            title_x=frame_width * 0.5,
            title_y=frame_height,
            font_size=font_size,
            text="",
        )

    metrics = measure_text(text, font_size)
    title_h = metrics.typo.height
    title_w = metrics.advance
    width = max(frame_width, title_w + 2 * PAD_PX)

    y_bot = occupancy.max_y()
    y_top = occupancy.min_y()
    if y_bot is None or y_top is None:
        # Empty drawing — just reserve the title band.
        height = title_h + TITLE_BOTTOM_PX + PAD_PX
        baseline = height - TITLE_BOTTOM_PX - metrics.typo.ymax
        return TitlePack(
            width=width,
            height=height,
            dy=0.0,
            title_x=width * 0.5,
            title_y=baseline,
            font_size=font_size,
            text=text,
        )

    # Ideal: mol bottom + clearance + title block + bottom gap.
    # Shift mol down to close slack under the ink (toward the title).
    content_bottom = y_bot
    title_block = title_h + TITLE_BOTTOM_PX
    height = content_bottom + TITLE_CLEARANCE_PX + title_block

    title_typo_top = height - TITLE_BOTTOM_PX - title_h
    # How far we can push the mol down before hitting the title clearance.
    dy = title_typo_top - TITLE_CLEARANCE_PX - content_bottom
    # Keep a little air at the top of the viewport.
    min_top = PAD_PX * 0.5
    if y_top + dy < min_top:
        dy = min_top - y_top
    if dy < 0:
        # Need more room below — grow the frame instead of overlapping.
        height -= dy
        dy = 0.0

    # After shifting, recompute height from the snug formula so we do not keep
    # a tall empty band under the molecule.
    content_bottom_shifted = content_bottom + dy
    height = content_bottom_shifted + TITLE_CLEARANCE_PX + title_block

    baseline = height - TITLE_BOTTOM_PX - metrics.typo.ymax
    return TitlePack(
        width=width,
        height=height,
        dy=dy,
        title_x=width * 0.5,
        title_y=baseline,
        font_size=font_size,
        text=text,
    )


def title_occupancy_box(
    pack: TitlePack,
) -> tuple[float, float, float, float]:
    """Typographic box for the packed title (for tests / debugging)."""
    box = text_box(
        pack.text,
        pack.title_x,
        pack.title_y,
        font_size=pack.font_size,
        anchor="middle",
        which="typo",
    )
    return box.as_tuple()


# Re-export tuning knobs used by callers / tests.
__all__ = [
    "TitlePack",
    "stamp_mol_occupancy",
    "pack_bottom_title",
    "title_occupancy_box",
]

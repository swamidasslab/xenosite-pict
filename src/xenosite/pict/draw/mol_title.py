"""Molecule caption (label) placement with occupancy snug-fit.

Default position is center-bottom; ``pos`` moves the caption to the other
edges (top / left / right). Clearance uses a coarse collision grid, not a
single bounding box.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from xenosite.pict.contracts.spec import LabelPos
from xenosite.pict.draw.collision import CollisionGrid
from xenosite.pict.draw.metrics import (
    PAD_PX,
    TITLE_BOTTOM_PX,
    TITLE_CLEARANCE_PX,
    TITLE_FONT_PX,
)
from xenosite.pict.draw.text_metrics import TextMetrics, measure_text, text_box

Anchor = Literal["start", "middle", "end"]


@dataclass(frozen=True)
class LabelPack:
    """Viewport size and content shift after packing a molecule label."""

    width: float
    height: float
    dx: float
    dy: float
    x: float  # text anchor x
    y: float  # baseline
    font_size: float
    text: str
    anchor: Anchor
    pos: LabelPos

    # Back-compat names from the bottom-only TitlePack era.
    @property
    def title_x(self) -> float:
        return self.x

    @property
    def title_y(self) -> float:
        return self.y


TitlePack = LabelPack


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


def _baseline_for_center(metrics: TextMetrics, center_y: float) -> float:
    """Baseline so the typographic box is vertically centered on ``center_y``."""
    return center_y - metrics.typo.cy


def _empty_pack(
    *,
    frame_width: float,
    frame_height: float,
    font_size: float,
    pos: LabelPos,
) -> LabelPack:
    return LabelPack(
        width=frame_width,
        height=frame_height,
        dx=0.0,
        dy=0.0,
        x=frame_width * 0.5,
        y=frame_height,
        font_size=font_size,
        text="",
        anchor="middle",
        pos=pos,
    )


def _pack_bottom(
    *,
    frame_width: float,
    occupancy: CollisionGrid,
    text: str,
    font_size: float,
    metrics: TextMetrics,
) -> LabelPack:
    title_h = metrics.typo.height
    title_w = metrics.advance
    width = max(frame_width, title_w + 2 * PAD_PX)
    edge = TITLE_BOTTOM_PX
    clear = TITLE_CLEARANCE_PX
    title_block = title_h + edge

    y_bot = occupancy.max_y()
    y_top = occupancy.min_y()
    if y_bot is None or y_top is None:
        height = title_block + PAD_PX
        baseline = height - edge - metrics.typo.ymax
        return LabelPack(
            width=width,
            height=height,
            dx=0.0,
            dy=0.0,
            x=width * 0.5,
            y=baseline,
            font_size=font_size,
            text=text,
            anchor="middle",
            pos=LabelPos.bottom,
        )

    height = y_bot + clear + title_block
    title_typo_top = height - edge - title_h
    dy = title_typo_top - clear - y_bot
    min_top = PAD_PX * 0.5
    if y_top + dy < min_top:
        dy = min_top - y_top
    if dy < 0:
        height -= dy
        dy = 0.0
    height = y_bot + dy + clear + title_block
    baseline = height - edge - metrics.typo.ymax
    return LabelPack(
        width=width,
        height=height,
        dx=0.0,
        dy=dy,
        x=width * 0.5,
        y=baseline,
        font_size=font_size,
        text=text,
        anchor="middle",
        pos=LabelPos.bottom,
    )


def _pack_top(
    *,
    frame_width: float,
    occupancy: CollisionGrid,
    text: str,
    font_size: float,
    metrics: TextMetrics,
) -> LabelPack:
    title_h = metrics.typo.height
    title_w = metrics.advance
    width = max(frame_width, title_w + 2 * PAD_PX)
    edge = TITLE_BOTTOM_PX
    clear = TITLE_CLEARANCE_PX
    title_block = title_h + edge
    baseline = edge - metrics.typo.ymin  # typo top sits ``edge`` below viewport top
    title_typo_bottom = edge + title_h

    y_bot = occupancy.max_y()
    y_top = occupancy.min_y()
    if y_bot is None or y_top is None:
        return LabelPack(
            width=width,
            height=title_block + PAD_PX,
            dx=0.0,
            dy=0.0,
            x=width * 0.5,
            y=baseline,
            font_size=font_size,
            text=text,
            anchor="middle",
            pos=LabelPos.top,
        )

    content_span = y_bot - y_top
    # Shift content toward the top label (negative dy closes slack).
    dy = title_typo_bottom + clear - y_top
    height = title_block + clear + content_span + PAD_PX * 0.5
    # Keep a little air at the bottom of the viewport.
    min_bottom_room = PAD_PX * 0.5
    if y_bot + dy + min_bottom_room > height:
        # Growing is already baked into the snug height; recompute from shifted bottom.
        height = y_bot + dy + min_bottom_room
    if dy > 0:
        # Overlap with the title band — frame grows; content moves down.
        height = title_typo_bottom + clear + content_span + min_bottom_room
        # dy already pushes content below the band.
    return LabelPack(
        width=width,
        height=height,
        dx=0.0,
        dy=dy,
        x=width * 0.5,
        y=baseline,
        font_size=font_size,
        text=text,
        anchor="middle",
        pos=LabelPos.top,
    )


def _pack_left(
    *,
    frame_width: float,
    frame_height: float,
    occupancy: CollisionGrid,
    text: str,
    font_size: float,
    metrics: TextMetrics,
) -> LabelPack:
    title_h = metrics.typo.height
    title_w = metrics.advance
    edge = TITLE_BOTTOM_PX
    clear = TITLE_CLEARANCE_PX
    label_block = title_w + edge

    x_min = occupancy.min_x()
    x_max = occupancy.max_x()
    y_bot = occupancy.max_y()
    y_top = occupancy.min_y()
    if x_min is None or x_max is None or y_bot is None or y_top is None:
        width = label_block + PAD_PX
        height = max(frame_height, title_h + 2 * PAD_PX)
        return LabelPack(
            width=width,
            height=height,
            dx=0.0,
            dy=0.0,
            x=edge + title_w * 0.5,
            y=_baseline_for_center(metrics, height * 0.5),
            font_size=font_size,
            text=text,
            anchor="middle",
            pos=LabelPos.left,
        )

    content_span = x_max - x_min
    # Shift content toward the left label (negative dx closes slack).
    dx = edge + title_w + clear - x_min
    width = max(frame_width, label_block + clear + content_span + PAD_PX * 0.5)
    min_right = PAD_PX * 0.5
    if x_max + dx + min_right > width:
        width = x_max + dx + min_right
    # With a wider-than-snug frame, recompute dx so content still clears the label.
    dx = edge + title_w + clear - x_min
    if x_max + dx + min_right > width:
        width = x_max + dx + min_right

    height = max(frame_height, title_h + 2 * PAD_PX, (y_bot - y_top) + 2 * PAD_PX)
    mid_y = 0.5 * (y_top + y_bot)
    baseline = _baseline_for_center(metrics, mid_y)
    if baseline + metrics.typo.ymin < PAD_PX * 0.5:
        baseline = PAD_PX * 0.5 - metrics.typo.ymin
    if baseline + metrics.typo.ymax > height - PAD_PX * 0.5:
        baseline = height - PAD_PX * 0.5 - metrics.typo.ymax

    return LabelPack(
        width=width,
        height=height,
        dx=dx,
        dy=0.0,
        x=edge + title_w * 0.5,
        y=baseline,
        font_size=font_size,
        text=text,
        anchor="middle",
        pos=LabelPos.left,
    )


def _pack_right(
    *,
    frame_width: float,
    frame_height: float,
    occupancy: CollisionGrid,
    text: str,
    font_size: float,
    metrics: TextMetrics,
) -> LabelPack:
    title_h = metrics.typo.height
    title_w = metrics.advance
    edge = TITLE_BOTTOM_PX
    clear = TITLE_CLEARANCE_PX
    label_block = title_w + edge

    x_min = occupancy.min_x()
    x_max = occupancy.max_x()
    y_bot = occupancy.max_y()
    y_top = occupancy.min_y()
    if x_min is None or x_max is None or y_bot is None or y_top is None:
        width = label_block + PAD_PX
        height = max(frame_height, title_h + 2 * PAD_PX)
        return LabelPack(
            width=width,
            height=height,
            dx=0.0,
            dy=0.0,
            x=width - edge - title_w * 0.5,
            y=_baseline_for_center(metrics, height * 0.5),
            font_size=font_size,
            text=text,
            anchor="middle",
            pos=LabelPos.right,
        )

    content_span = x_max - x_min
    # Mirror of bottom: place label on the right, snug content toward it.
    width = content_span + clear + label_block + PAD_PX * 0.5
    label_left = width - edge - title_w
    dx = label_left - clear - x_max
    min_left = PAD_PX * 0.5
    if x_min + dx < min_left:
        dx = min_left - x_min
    if dx < 0:
        # Slack — pulling toward label; snug width from shifted content.
        width = (x_max + dx) + clear + label_block
        label_left = width - edge - title_w
        dx = label_left - clear - x_max
    else:
        # Need room on the right — grow.
        width = x_max + dx + clear + label_block
        label_left = width - edge - title_w

    # Final consistency: content right + clear == label left.
    width = max(frame_width, x_max + dx + clear + label_block)
    label_left = width - edge - title_w
    dx = label_left - clear - x_max
    if x_min + dx < min_left:
        width += min_left - (x_min + dx)
        label_left = width - edge - title_w
        dx = label_left - clear - x_max

    height = max(frame_height, title_h + 2 * PAD_PX, (y_bot - y_top) + 2 * PAD_PX)
    mid_y = 0.5 * (y_top + y_bot)
    baseline = _baseline_for_center(metrics, mid_y)
    if baseline + metrics.typo.ymin < PAD_PX * 0.5:
        baseline = PAD_PX * 0.5 - metrics.typo.ymin
    if baseline + metrics.typo.ymax > height - PAD_PX * 0.5:
        baseline = height - PAD_PX * 0.5 - metrics.typo.ymax

    return LabelPack(
        width=width,
        height=height,
        dx=dx,
        dy=0.0,
        x=width - edge - title_w * 0.5,
        y=baseline,
        font_size=font_size,
        text=text,
        anchor="middle",
        pos=LabelPos.right,
    )


def pack_label(
    *,
    frame_width: float,
    frame_height: float,
    occupancy: CollisionGrid,
    text: str,
    pos: LabelPos | str = LabelPos.bottom,
    font_size: float = TITLE_FONT_PX,
) -> LabelPack:
    """Place a molecule caption and snug the drawing toward it."""
    label_pos = LabelPos(pos) if not isinstance(pos, LabelPos) else pos
    stripped = text.strip()
    if not stripped:
        return _empty_pack(
            frame_width=frame_width,
            frame_height=frame_height,
            font_size=font_size,
            pos=label_pos,
        )
    metrics = measure_text(stripped, font_size)
    if label_pos is LabelPos.bottom:
        return _pack_bottom(
            frame_width=frame_width,
            occupancy=occupancy,
            text=stripped,
            font_size=font_size,
            metrics=metrics,
        )
    if label_pos is LabelPos.top:
        return _pack_top(
            frame_width=frame_width,
            occupancy=occupancy,
            text=stripped,
            font_size=font_size,
            metrics=metrics,
        )
    if label_pos is LabelPos.left:
        return _pack_left(
            frame_width=frame_width,
            frame_height=frame_height,
            occupancy=occupancy,
            text=stripped,
            font_size=font_size,
            metrics=metrics,
        )
    if label_pos is LabelPos.right:
        return _pack_right(
            frame_width=frame_width,
            frame_height=frame_height,
            occupancy=occupancy,
            text=stripped,
            font_size=font_size,
            metrics=metrics,
        )
    raise ValueError(f"unsupported label pos: {label_pos!r}")


def pack_bottom_title(
    *,
    frame_width: float,
    frame_height: float,
    occupancy: CollisionGrid,
    title: str,
    font_size: float = TITLE_FONT_PX,
) -> LabelPack:
    """Back-compat wrapper for center-bottom packing."""
    return pack_label(
        frame_width=frame_width,
        frame_height=frame_height,
        occupancy=occupancy,
        text=title,
        pos=LabelPos.bottom,
        font_size=font_size,
    )


def label_occupancy_box(
    pack: LabelPack,
) -> tuple[float, float, float, float]:
    """Typographic box for the packed label (for tests / debugging)."""
    box = text_box(
        pack.text,
        pack.x,
        pack.y,
        font_size=pack.font_size,
        anchor=pack.anchor,
        which="typo",
    )
    return box.as_tuple()


def title_occupancy_box(pack: LabelPack) -> tuple[float, float, float, float]:
    """Back-compat alias for :func:`label_occupancy_box`."""
    return label_occupancy_box(pack)


__all__ = [
    "LabelPack",
    "TitlePack",
    "stamp_mol_occupancy",
    "pack_label",
    "pack_bottom_title",
    "label_occupancy_box",
    "title_occupancy_box",
]

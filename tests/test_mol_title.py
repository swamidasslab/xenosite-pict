"""Molecule title packing and collision-grid helpers."""

from __future__ import annotations

import re

import pytest

from xenosite.pict import Pict, render
from xenosite.pict.draw.collision import CollisionGrid
from xenosite.pict.draw.metrics import (
    COLLISION_CELL_PX,
    TITLE_BOTTOM_PX,
    TITLE_CLEARANCE_PX,
    TITLE_FONT_PX,
)
from xenosite.pict.draw.mol_title import pack_bottom_title, title_occupancy_box
from xenosite.pict.draw.scene_builder import viewport_size
from xenosite.pict.draw.text_metrics import measure_text


def test_collision_grid_marks_segment_and_box():
    g = CollisionGrid(cell=4.0)
    g.mark_segment(0, 0, 20, 0, radius=2.0)
    g.mark_box(30, 30, 40, 40)
    assert g.min_y() is not None and g.max_y() is not None
    assert g.max_x() > 20
    assert g.overlaps_box(32, 32, 36, 36)
    assert not g.overlaps_box(100, 100, 110, 110)


def test_pack_bottom_title_is_center_bottom_with_clearance():
    grid = CollisionGrid(cell=COLLISION_CELL_PX)
    # Fake mol ink occupying y=10..40 in a 80×80 frame.
    grid.mark_box(10, 10, 70, 40)
    pack = pack_bottom_title(
        frame_width=80,
        frame_height=80,
        occupancy=grid,
        title="ethanol",
    )
    assert pack.text == "ethanol"
    assert pack.font_size == TITLE_FONT_PX
    assert pack.title_x == pytest.approx(pack.width * 0.5)
    typo = title_occupancy_box(pack)
    # Typo bottom sits TITLE_BOTTOM_PX above the viewport bottom.
    assert pack.height - typo[3] == pytest.approx(TITLE_BOTTOM_PX, abs=COLLISION_CELL_PX)
    # Mol ink (shifted) clears the title by about TITLE_CLEARANCE_PX.
    mol_bottom = 40 + pack.dy
    assert typo[1] - mol_bottom == pytest.approx(TITLE_CLEARANCE_PX, abs=COLLISION_CELL_PX + 0.5)


def test_title_pack_reserves_caption_band():
    backend = "native"
    pict = Pict(backend=backend)
    bare = pict.layout({"molecules": [{"smiles": "CCO"}]}).molecules[0]
    from xenosite.pict.contracts.spec import MoleculeSpec

    mol = MoleculeSpec(smiles="CCO", title="ethanol")
    w1, h1 = viewport_size(bare, mol)
    metrics = measure_text("ethanol", TITLE_FONT_PX)
    # Packed height must fit ink clearance + title typo + bottom gap.
    assert h1 >= TITLE_CLEARANCE_PX + metrics.typo.height + TITLE_BOTTOM_PX
    assert w1 >= metrics.advance


def test_render_emits_centered_mol_title():
    svg = render(
        {"molecules": [{"smiles": "CCO", "title": "ethanol"}]},
        backend="native",
    )
    m = re.search(
        r'<text([^>]*)class="mol-title"([^>]*)>([^<]*)</text>',
        svg,
    )
    assert m is not None
    attrs = m.group(1) + m.group(2)
    assert m.group(3) == "ethanol"
    assert f'font-size="{TITLE_FONT_PX}"' in attrs or f"font-size=\"{TITLE_FONT_PX}" in attrs
    assert 'text-anchor="middle"' in attrs
    height = float(re.search(r'height="([0-9.]+)"', svg).group(1))
    title_y = float(re.search(r'\by="([0-9.]+)"', attrs).group(1))
    assert title_y > height * 0.5


def test_title_snug_is_shorter_than_naive_pad_stack():
    """Snug packing should beat frame_height + full title band with no trim."""
    grid = CollisionGrid(cell=COLLISION_CELL_PX)
    grid.mark_box(10, 10, 70, 50)
    pack = pack_bottom_title(
        frame_width=100,
        frame_height=100,
        occupancy=grid,
        title="aspirin",
    )
    metrics = measure_text("aspirin", TITLE_FONT_PX)
    naive = 100 + TITLE_CLEARANCE_PX + metrics.typo.height + TITLE_BOTTOM_PX
    assert pack.height < naive

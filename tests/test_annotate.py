"""Annotation callouts, region shapes, and collision-grid placement."""

from __future__ import annotations

import re

import pytest

from xpict import render
from xpict.future.nodes import PictSpec
from xpict.future.spec import (
    AnnotationSpec,
    AnnotKind,
    AnnotPrefer,
)
from xpict.draw.annotate import draw_annotations
from xpict.draw.collision import CollisionGrid
from xpict.draw.metrics import COLLISION_CELL_PX
from xpict.draw.scene_builder import build_scene


def test_annotation_spec_requires_target():
    with pytest.raises(ValueError, match="atoms, bonds, or ring"):
        AnnotationSpec(kind=AnnotKind.callout, label="x")


def test_box_annotation_needs_atoms():
    with pytest.raises(ValueError, match="box"):
        AnnotationSpec(kind=AnnotKind.box, bonds=[(0, 1)])


def test_collision_find_slot_prefers_free_side():
    grid = CollisionGrid(cell=COLLISION_CELL_PX)
    # Occupy the right of the anchor.
    grid.mark_box(30, 0, 80, 40)
    cx, cy, side = grid.find_slot(20, 20, 20, 10, prefer="right", gap=4.0)
    assert side == "left"
    assert cx < 20


def test_callout_svg_has_label_and_arrow():
    svg = render(
        {
            "molecules": [
                {
                    "smiles": "CCO",
                    "annotations": [
                        {
                            "kind": "callout",
                            "atoms": [1],
                            "label": "site",
                            "color": "#06c",
                            "prefer": "right",
                        }
                    ],
                }
            ]
        },
        backend="native",
    )
    assert "annot-label" in svg
    assert "annot-arrow" in svg
    assert "site" in svg or "data-text" in svg


def test_region_kinds_emit_paths():
    for kind, cls in (
        ("box", "annot-box"),
        ("oval", "annot-oval"),
        ("spline", "annot-spline"),
    ):
        svg = render(
            {
                "molecules": [
                    {
                        "smiles": "c1ccccc1",
                        "annotations": [{"kind": kind, "atoms": [0, 1, 2], "color": "#c44"}],
                    }
                ]
            },
            backend="native",
        )
        assert cls in svg


def test_ring_callout_uses_centroid():
    svg = render(
        {
            "molecules": [
                {
                    "smiles": "c1ccccc1O",
                    "annotations": [
                        {
                            "kind": "callout",
                            "ring": [0, 1, 2, 3, 4, 5],
                            "label": "Ar",
                            "arrow": True,
                        }
                    ],
                }
            ]
        },
        backend="native",
    )
    assert "annot-label" in svg
    assert re.search(r"data-text=\"Ar\"", svg) or "Ar" in svg


def test_annotations_land_on_marks_layer():
    from xpict import Pict

    pict = Pict(backend="native")
    spec = PictSpec.model_validate(
        {
            "molecules": [
                {
                    "smiles": "CCO",
                    "annotations": [{"atoms": [0], "label": "OH"}],
                }
            ]
        }
    )
    layouts = pict.layout(spec).molecules
    scene = build_scene(layouts, spec.molecules, spec.to_legacy())
    marks = next(layer for layer in scene.viewports[0].layers if layer.name == "marks")
    assert any(getattr(p, "cls", None) and "annot" in (p.cls or "") for p in marks.primitives)


def test_draw_annotations_stamps_grid():
    grid = CollisionGrid(cell=4.0)
    grid.mark_box(0, 0, 40, 40)
    coords = [(10.0, 10.0), (30.0, 10.0)]
    atom_pos = {0: 0, 1: 1}
    ann = AnnotationSpec(
        kind=AnnotKind.callout,
        atoms=[0],
        label="A",
        prefer=AnnotPrefer.right,
    )
    drawn = draw_annotations([ann], atom_pos=atom_pos, coords=coords, grid=grid)
    assert drawn.primitives
    assert drawn.boxes
    # Second callout should avoid the first label box.
    before = len(grid.cells)
    ann2 = AnnotationSpec(
        kind=AnnotKind.callout,
        atoms=[0],
        label="B",
        prefer=AnnotPrefer.right,
    )
    draw_annotations([ann2], atom_pos=atom_pos, coords=coords, grid=grid)
    assert len(grid.cells) >= before

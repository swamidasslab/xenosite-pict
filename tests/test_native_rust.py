"""Parity tests for optional ``xpict._native`` (maturin / Rust core)."""

from __future__ import annotations

import math

import pytest
from shapely.geometry import LineString, Point

from xpict.native_bridge import HAS_RUST_CORE, CapsuleInk, multi_bond_offset
from xpict.draw.bonds import _multi_bond_offset_py
from xpict.draw.halo import capsule_shape, halo_path_d
from xpict.draw.metrics import LABEL_GAP_PX, OFFSET_PX
from xpict.draw.plotdot import PlotDot


pytestmark = pytest.mark.skipif(not HAS_RUST_CORE, reason="xpict._native not built (run maturin develop)")


def test_multi_bond_offset_matches_python():
    for length in (3.0, 10.0, 20.0, 100.0):
        assert multi_bond_offset(length) == pytest.approx(_multi_bond_offset_py(length))


def test_plotdot_rings_match_python():
    from xpict import _native

    pd = PlotDot()
    for z in (0.25, 0.5, 0.9, 1.0):
        py_rings = pd.rings(z)
        rs_rings = _native.plotdot_rings(z, 4)
        assert len(py_rings) == len(rs_rings)
        for (r1, c1), (r2, c2) in zip(py_rings, rs_rings, strict=True):
            assert r1 == pytest.approx(r2, abs=1e-9)
            assert c1 == pytest.approx(c2, abs=1e-9)


def test_capsule_halo_near_shapely():
    from shapely.geometry import Polygon

    from xpict import _native
    from xpict.draw.paths import path_coords

    x1, y1, x2, y2 = 0.0, 0.0, 20.0, 0.0
    ink_r = 0.56
    grow = LABEL_GAP_PX
    rust_d = _native.capsule_halo_path_d(x1, y1, x2, y2, ink_r, grow)
    assert rust_d and rust_d.endswith("Z")
    ink = LineString([(x1, y1), (x2, y2)]).buffer(ink_r, quad_segs=8, cap_style=1)
    shapely_d = halo_path_d(ink, grow)
    assert shapely_d
    rust_area = Polygon(path_coords(rust_d)).area
    shapely_area = Polygon(path_coords(shapely_d)).area
    assert rust_area == pytest.approx(shapely_area, rel=0.08)


def test_tagged_capsule_uses_rust_in_halo_path_d():
    ink = capsule_shape(0.0, 0.0, 20.0, 0.0, 0.56)
    assert isinstance(ink, CapsuleInk)
    d = halo_path_d(ink, LABEL_GAP_PX)
    assert d and "M" in d and d.endswith("Z")


def test_offset_px_constant():
    from xpict import _native

    assert _native.OFFSET_PX == pytest.approx(OFFSET_PX)


def test_elk_layout_json_layered():
    import json

    from xpict import _native

    assert getattr(_native, "HAS_ELK", False)
    graph = {
        "id": "root",
        "layoutOptions": {
            "elk.algorithm": "layered",
            "elk.direction": "RIGHT",
            "elk.edgeRouting": "ORTHOGONAL",
        },
        "children": [
            {"id": "a", "width": 40.0, "height": 30.0},
            {"id": "b", "width": 40.0, "height": 30.0},
        ],
        "edges": [{"id": "e0", "sources": ["a"], "targets": ["b"]}],
    }
    laid = json.loads(_native.elk_layout_json(json.dumps(graph)))
    by_id = {c["id"]: c for c in laid["children"]}
    assert by_id["b"]["x"] > by_id["a"]["x"]
    edge = laid["edges"][0]
    assert edge["sections"]
    assert "startPoint" in edge["sections"][0]

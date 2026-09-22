"""PyO3 ABI smoke for ``xpict._native``.

Numeric / geometry parity with Python helpers lives in
``cargo test -p xpict-core``. This module only checks the extension module
still exports the surface Python imports.
"""

from __future__ import annotations

import json

import pytest

from xpict.draw.metrics import LABEL_GAP_PX, OFFSET_PX
from xpict.native_bridge import HAS_RUST_CORE, CapsuleInk, Shape, multi_bond_offset

pytestmark = pytest.mark.skipif(
    not HAS_RUST_CORE, reason="xpict._native not built (run maturin develop)"
)


def test_native_exports_constants_and_helpers():
    from xpict import _native

    assert _native.OFFSET_PX == pytest.approx(OFFSET_PX)
    assert multi_bond_offset(20.0) > 0.0
    d = _native.capsule_halo_path_d(0.0, 0.0, 20.0, 0.0, 0.56, LABEL_GAP_PX)
    assert d and d.endswith("Z")
    assert isinstance(CapsuleInk(0.0, 0.0, 20.0, 0.0, 0.56), CapsuleInk)


def test_native_shape_and_elk_abi():
    from xpict import _native

    assert getattr(_native, "HAS_GEOM", False)
    outer = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]
    inner = [(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)]
    ink = Shape.from_contours_evenodd([outer, inner])
    assert not ink.is_empty and ink.has_holes

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

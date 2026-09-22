"""Binding smoke for ``xpict._native.depict_molecule``.

Core geometry / paint asserts live in ``cargo test -p xpict-core``
(see ``crates/xpict-core/src/depict.rs`` tests). Keep this file thin so
Python CI still exercises the PyO3 ABI without duplicating Rust coverage.
"""

from __future__ import annotations

import json

import pytest

from xpict.draw.metrics import PAD_PX

try:
    from xpict import _native

    HAS_RUST = True
except ImportError:
    HAS_RUST = False

pytestmark = pytest.mark.skipif(
    not HAS_RUST, reason="xpict._native not built (run maturin develop)"
)


def test_native_depict_molecule_abi():
    mol = {
        "id": "etoh",
        "atoms": [
            {"index": 0, "element": "C", "x": 0.0, "y": 0.0},
            {"index": 1, "element": "C", "x": 20.0, "y": 0.0},
            {"index": 2, "element": "O", "x": 30.0, "y": 10.0, "label": "OH"},
        ],
        "bonds": [
            {"index": 0, "begin": 0, "end": 1, "order": 1.0},
            {"index": 1, "begin": 1, "end": 2, "order": 1.0},
        ],
    }
    scene = json.loads(_native.depict_molecule(json.dumps(mol)))
    assert scene["viewports"][0]["id"] == "etoh"
    assert scene["width"] > PAD_PX and scene["height"] > PAD_PX
    layers = {layer["name"]: layer for layer in scene["viewports"][0]["layers"]}
    assert "bonds" in layers
    assert len(layers["bonds"]["primitives"]) == 2
    labels = layers["labels"]["primitives"]
    texts = [p.get("data_text") for p in labels if p.get("data_text")]
    assert any(t in ("OH", "HO") for t in texts), texts

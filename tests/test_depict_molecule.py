"""Parity: Rust ``depict_molecule`` / bond joins vs Python stroke helpers."""

from __future__ import annotations

import json
import math
import re

import pytest

from xpict.draw.bonds import DrawnBond, bond_strokes, join_centered_multibonds
from xpict.draw.metrics import OFFSET_PX

try:
    from xpict import _native

    HAS_RUST = True
except ImportError:
    HAS_RUST = False

pytestmark = pytest.mark.skipif(
    not HAS_RUST, reason="xpict._native not built (run maturin develop)"
)


def _pts(d: str) -> list[tuple[float, float]]:
    return [(float(x), float(y)) for x, y in re.findall(r"[ML]\s+([-\d.]+)\s+([-\d.]+)", d)]


def _on_line(px: float, py: float, x1: float, y1: float, x2: float, y2: float) -> bool:
    cross = abs((x2 - x1) * (py - y1) - (y2 - y1) * (px - x1))
    length = math.hypot(x2 - x1, y2 - y1) or 1.0
    return cross / length < 0.08


def test_depict_molecule_ethanol_two_bonds():
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
    bonds = next(l for l in scene["viewports"][0]["layers"] if l["name"] == "bonds")
    assert len(bonds["primitives"]) == 2
    assert scene["width"] > 0 and scene["height"] > 0


def test_depict_acetone_centered_double_offsets():
    mol = {
        "id": "acetone",
        "atoms": [
            {"index": 0, "element": "C", "x": -20.0, "y": 8.0},
            {"index": 1, "element": "C", "x": 0.0, "y": 0.0},
            {"index": 2, "element": "O", "x": 0.0, "y": -20.0, "label": "O"},
            {"index": 3, "element": "C", "x": 20.0, "y": 8.0},
        ],
        "bonds": [
            {"index": 0, "begin": 0, "end": 1, "order": 1.0},
            {"index": 1, "begin": 1, "end": 2, "order": 2.0},
            {"index": 2, "begin": 1, "end": 3, "order": 1.0},
        ],
        "color": "#336699",
    }
    scene = json.loads(_native.depict_molecule(json.dumps(mol)))
    bonds = next(l for l in scene["viewports"][0]["layers"] if l["name"] == "bonds")
    offsets = [
        p
        for p in bonds["primitives"]
        if "bond-offset" in (p.get("cls") or "")
    ]
    assert len(offsets) >= 2
    assert all(p.get("stroke") == "#336699" for p in bonds["primitives"])


def test_depict_marks_and_shade_layers():
    mol = {
        "atoms": [
            {"index": 0, "element": "C", "x": 0.0, "y": 0.0},
            {"index": 1, "element": "O", "x": 20.0, "y": 0.0, "label": "O"},
        ],
        "bonds": [{"index": 0, "begin": 0, "end": 1, "order": 1.0}],
        "mark_atoms": [1],
        "atom_shade": [0.0, 0.85],
    }
    scene = json.loads(_native.depict_molecule(json.dumps(mol)))
    names = {l["name"] for l in scene["viewports"][0]["layers"]}
    assert names >= {"shading", "bonds", "marks"}


def test_python_join_two_singles_matches_rust_geometry():
    """Same fixture as Rust ``two_singles_meet_and_clip_the_double``."""
    bonds = [
        DrawnBond(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
        DrawnBond(1, 0, 2, 0.0, 0.0, -10.0, 8.0, 1.0),
        DrawnBond(2, 0, 3, 0.0, 0.0, -10.0, -8.0, 1.0),
    ]
    join_centered_multibonds(bonds)
    assert bonds[0].trims is not None
    assert all(t < 0.0 for t in bonds[0].trims[0])
    strokes = bond_strokes(0, 0, 20, 0, 2.0, trims=bonds[0].trims)
    for path in strokes.offsets:
        x, y = _pts(path.d)[0]
        assert _on_line(x, y, 0.0, 0.0, -10.0, 8.0) or _on_line(
            x, y, 0.0, 0.0, -10.0, -8.0
        )


def test_rust_acetone_double_has_joined_offsets():
    """Rust depict paints centered C=O as two offset strokes (join applied)."""
    mol = {
        "atoms": [
            {"index": 0, "element": "C", "x": -20.0, "y": 8.0},
            {"index": 1, "element": "C", "x": 0.0, "y": 0.0},
            {"index": 2, "element": "O", "x": 0.0, "y": -20.0, "label": "O"},
            {"index": 3, "element": "C", "x": 20.0, "y": 8.0},
        ],
        "bonds": [
            {"index": 0, "begin": 0, "end": 1, "order": 1.0},
            {"index": 1, "begin": 1, "end": 2, "order": 2.0},
            {"index": 2, "begin": 1, "end": 3, "order": 1.0},
        ],
    }
    py_bonds = [
        DrawnBond(0, 0, 1, -20.0, 8.0, 0.0, 0.0, 1.0),
        DrawnBond(
            1, 1, 2, 0.0, 0.0, 0.0, -20.0, 2.0, begin_labeled=False, end_labeled=True
        ),
        DrawnBond(2, 1, 3, 0.0, 0.0, 20.0, 8.0, 1.0),
    ]
    join_centered_multibonds(py_bonds)
    assert py_bonds[1].trims is not None
    assert all(t < 0.0 for t in py_bonds[1].trims[0])

    scene = json.loads(_native.depict_molecule(json.dumps(mol)))
    bonds = next(l for l in scene["viewports"][0]["layers"] if l["name"] == "bonds")
    offsets = [p for p in bonds["primitives"] if "bond-offset" in (p.get("cls") or "")]
    assert len(offsets) >= 2
    assert OFFSET_PX == pytest.approx(_native.OFFSET_PX)

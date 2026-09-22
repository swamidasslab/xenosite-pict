"""POC-focused tests: marks/shade drawing and ELK graph synthesis."""

from __future__ import annotations

import json
import warnings

from xpict import Pict, render
from xpict.future.nodes import PictSpec
from xpict.diagram.elk import elk_graph, elk_graph_json, layout_diagram


def _chem_backend() -> str:
    """MVP layout backend (indigo is out of scope for now)."""
    return "native"


def test_own_svg_has_bonds_and_labels():
    backend = _chem_backend()
    svg = render({"molecules": [{"smiles": "CCO"}]}, backend=backend)
    assert "<svg" in svg
    assert 'class="bond' in svg or "bond-" in svg
    assert "O" in svg  # heteroatom label


def test_shade_and_marks_layers():
    backend = _chem_backend()
    pict = Pict(backend=backend)
    n = len(pict.layout({"molecules": [{"smiles": "CCO"}]}).molecules[0].atoms)
    svg = pict.render(
        {
            "molecules": [
                {
                    "smiles": "CCO",
                    "shade": {"atoms": [0.2 * i for i in range(n)]},
                    "marks": [
                        {"kind": "atoms", "atoms": [n - 1], "color": "#c00"},
                        {"kind": "substructure", "atoms": list(range(n)), "color": "#06c"},
                    ],
                }
            ]
        }
    )
    assert "shade" in svg
    assert "substructure-mark" in svg or "mark" in svg


def test_elk_graph_uses_molecule_ids():
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "C"},
            ],
            "diagram": {
                "kind": "network",
                "edges": [{"source": "A", "target": "B"}],
            },
        }
    )
    pict = Pict(backend="native")
    layouts = pict.layout(doc).molecules
    graph = elk_graph(layouts, doc)
    ids = {c["id"] for c in graph["children"]}
    assert ids == {"A", "B"}
    assert graph["edges"][0]["sources"] == ["A"]
    assert json.loads(elk_graph_json(layouts, doc))["id"] == "root"


def test_network_layout_positions_length():
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "C"},
                {"id": "C", "smiles": "O"},
            ],
            "diagram": {
                "kind": "network",
                "edges": [
                    {"source": "A", "target": "B"},
                    {"source": "B", "target": "C"},
                ],
            },
        }
    )
    layouts = Pict(backend="native").layout(doc).molecules
    with warnings.catch_warnings(record=True):
        warnings.simplefilter("always")
        positions = layout_diagram(layouts, doc)
    assert len(positions) == 3


def test_elk_places_network():
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "A", "smiles": "CCO"},
                {"id": "B", "smiles": "C"},
                {"id": "C", "smiles": "CC=O"},
            ],
            "diagram": {
                "kind": "network",
                "edges": [
                    {"source": "A", "target": "B"},
                    {"source": "B", "target": "C"},
                ],
            },
        }
    )
    layouts = Pict(backend="native").layout(doc).molecules
    positions = layout_diagram(layouts, doc)
    assert len(positions) == 3
    # elkrs should separate nodes (not all stacked at origin).
    assert len({(round(x, 1), round(y, 1)) for x, y in positions}) >= 2

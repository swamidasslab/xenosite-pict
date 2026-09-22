"""Nested PictSpec object algebra: lift, flatten, validate."""

from __future__ import annotations

import pytest

from xpict import render
from xpict.future.nodes import (
    ArrowNode,
    GridNode,
    MolNode,
    PictSpec,
    ReactionNode,
    lift_legacy,
)
from xpict.future.spec import EdgeArrow


def test_legacy_single_mol_lifts_to_mol_root():
    node = lift_legacy({"molecules": [{"smiles": "CCO", "label": "et"}]})
    assert node["type"] == "mol"
    assert node["smiles"] == "CCO"
    assert node["layout"]["label"]["text"] == "et"


def test_legacy_reaction_lifts_to_reaction_container():
    node = lift_legacy(
        {
            "molecules": [
                {"id": "a", "smiles": "CCO"},
                {"id": "b", "smiles": "C"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [{"source": "a", "target": "b", "label": "MeI", "arrow": "forward"}],
            },
        }
    )
    assert node["type"] == "reaction"
    assert len(node["children"]) == 2
    assert node["layout"]["edges"][0]["label"] == "MeI"


def test_nested_reaction_flattens_arrow_above_to_edge_label():
    tree = PictSpec.model_validate(
        {
            "type": "reaction",
            "children": [
                {"type": "mol", "id": "a", "smiles": "CCO"},
                {
                    "type": "arrow",
                    "arrow": "forward",
                    "layout": {"above": ["MeI"]},
                },
                {"type": "mol", "id": "b", "smiles": "C"},
            ],
        }
    )
    assert isinstance(tree.root, ReactionNode)
    leg = tree.to_legacy()
    assert leg.diagram.kind.value == "reaction"
    assert len(leg.molecules) == 2
    assert len(leg.diagram.edges) == 1
    assert leg.diagram.edges[0].label == "MeI"
    assert leg.diagram.edges[0].arrow == EdgeArrow.forward


def test_grid_with_panel_and_table_validates():
    tree = PictSpec.model_validate(
        {
            "type": "grid",
            "layout": {"columns": 2, "output": "children"},
            "children": [
                {
                    "type": "mol",
                    "panel": "a",
                    "smiles": "*C",
                    "rgroups": ["R"],
                    "rtable": {
                        "groups": ["R"],
                        "rows": [["Me"], ["Et"]],
                    },
                },
                {"type": "image", "panel": "b", "src": "nmr.png"},
            ],
        }
    )
    assert isinstance(tree.root, GridNode)
    mol = tree.root.children[0]
    assert isinstance(mol, MolNode)
    assert mol.panel == "a"
    assert mol.rtable is not None
    assert not hasattr(mol, "children") or "children" not in MolNode.model_fields


def test_mol_rejects_children():
    with pytest.raises(Exception):
        PictSpec.model_validate(
            {
                "type": "mol",
                "smiles": "CCO",
                "children": [{"type": "table", "rows": []}],
            }
        )


def test_roundtrip_legacy_via_pictspec():
    raw = {
        "molecules": [{"smiles": "c1ccccc1", "label": "PhH"}],
        "halo": False,
    }
    tree = PictSpec.model_validate(raw)
    leg = tree.to_legacy()
    assert leg.halo is False
    assert leg.molecules[0].smiles == "c1ccccc1"
    assert leg.molecules[0].label is not None
    assert leg.molecules[0].label.text == "PhH"


def test_render_nested_mol():
    svg = render({"type": "mol", "smiles": "CCO"}, backend="native")
    assert "<svg" in svg


def test_mol_node_requires_structure():
    with pytest.raises(ValueError, match="smiles"):
        MolNode.model_validate({"type": "mol"})


def test_arrow_node_defaults():
    a = ArrowNode.model_validate({"type": "arrow"})
    assert a.arrow == EdgeArrow.forward

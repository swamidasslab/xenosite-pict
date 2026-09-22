"""JSON Schema minify / dedupe into $refs."""

from __future__ import annotations

import json

from xpict.future.nodes import PictSpec
from xpict.export_schema import minify_json_schema


def test_minify_reduces_size_and_shares_children():
    from xpict.export_schema import factor_node_common_allof

    raw = PictSpec.model_json_schema()
    mini = minify_json_schema(factor_node_common_allof(raw))
    raw_s = json.dumps(raw, indent=2)
    mini_s = json.dumps(mini, indent=2)
    assert len(mini_s) < len(raw_s) * 0.85

    assert isinstance(mini["oneOf"], list)
    assert isinstance(mini["discriminator"], dict)

    defs = mini["$defs"]
    assert "NodeCommon" in defs
    assert "ContainerCommon" in defs
    mol = defs["MolNode"]
    assert any(
        isinstance(x, dict) and x.get("$ref") == "#/$defs/NodeCommon" for x in mol.get("allOf", [])
    )
    blob = json.dumps(mini)
    assert blob.count("#/$defs/NodeCommon") >= 1
    assert blob.count("#/$defs/ContainerCommon") >= 5
    # children only on ContainerCommon, not on leaves
    assert "children" in defs["ContainerCommon"]["allOf"][1]["properties"]
    assert "children" not in defs["NodeCommon"].get("properties", {})


def test_minify_preserves_def_bodies():
    from xpict.export_schema import factor_node_common_allof

    mini = minify_json_schema(factor_node_common_allof(PictSpec.model_json_schema()))
    assert "NodeCommon" in mini["$defs"]
    mol = mini["$defs"]["MolNode"]
    assert "allOf" in mol
    # Containers share ContainerCommon (NodeCommon + children)
    assert "ContainerCommon" in mini["$defs"]
    grid = mini["$defs"]["GridNode"]
    assert any(
        isinstance(x, dict) and x.get("$ref") == "#/$defs/ContainerCommon"
        for x in grid.get("allOf", [])
    )
    # Leaves must not declare children
    mol_blob = json.dumps(mol)
    assert "children" not in mol_blob or "#/$defs/NodeCommon" in mol_blob
    assert "children" not in json.dumps(
        [x for x in mol["allOf"] if isinstance(x, dict) and "properties" in x]
    )

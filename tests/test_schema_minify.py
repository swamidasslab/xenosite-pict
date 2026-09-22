"""JSON Schema minify / dedupe into $refs."""

from __future__ import annotations

import json

from xpict.contracts.nodes import PictSpec
from xpict.export_schema import minify_json_schema


def test_minify_reduces_size_and_shares_children():
    from xpict.export_schema import factor_node_common_allof

    raw = PictSpec.model_json_schema()
    mini = minify_json_schema(factor_node_common_allof(raw))
    raw_s = json.dumps(raw, indent=2)
    mini_s = json.dumps(mini, indent=2)
    assert len(mini_s) < len(raw_s) * 0.75

    assert isinstance(mini["oneOf"], list)
    assert isinstance(mini["discriminator"], dict)

    defs = mini["$defs"]
    assert "NodeCommon" in defs
    mol = defs["MolNode"]
    assert any(
        isinstance(x, dict) and x.get("$ref") == "#/$defs/NodeCommon"
        for x in mol.get("allOf", [])
    )
    blob = json.dumps(mini)
    assert blob.count("#/$defs/NodeCommon") >= 12
    # Shared children live on NodeCommon (once), not re-inlined per node
    assert "children" in defs["NodeCommon"].get("properties", {})


def test_minify_preserves_def_bodies():
    from xpict.export_schema import factor_node_common_allof

    mini = minify_json_schema(factor_node_common_allof(PictSpec.model_json_schema()))
    assert "NodeCommon" in mini["$defs"]
    assert "properties" in mini["$defs"]["NodeCommon"]
    mol = mini["$defs"]["MolNode"]
    assert "allOf" in mol

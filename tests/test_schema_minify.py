"""JSON Schema minify / dedupe into $refs."""

from __future__ import annotations

import json

from xpict.contracts.nodes import PictSpec
from xpict.export_schema import minify_json_schema


def test_minify_reduces_size_and_shares_children():
    raw = PictSpec.model_json_schema()
    mini = minify_json_schema(raw)
    raw_s = json.dumps(raw, indent=2)
    mini_s = json.dumps(mini, indent=2)
    assert len(mini_s) < len(raw_s) * 0.85

    # Structural validity: keyword shapes preserved
    assert isinstance(mini["oneOf"], list)
    assert isinstance(mini["discriminator"], dict)
    assert mini["discriminator"].get("propertyName") == "type"

    defs = mini["$defs"]
    assert "Children" in defs
    assert defs["Children"]["type"] == "array"
    # Shared across node kinds
    blob = json.dumps(mini)
    assert blob.count("#/$defs/Children") >= 8
    assert defs["MolNode"]["properties"]["children"] == {"$ref": "#/$defs/Children"}
    assert defs["GridNode"]["properties"]["children"] == {"$ref": "#/$defs/Children"}


def test_minify_preserves_def_bodies():
    mini = minify_json_schema(PictSpec.model_json_schema())
    for name in ("MolNode", "LayoutSpec", "ReactionNode"):
        body = mini["$defs"][name]
        assert isinstance(body, dict)
        assert "$ref" not in body
        assert "properties" in body

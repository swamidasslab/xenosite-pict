"""Shorthand expand/compress and tree-map helpers."""

from __future__ import annotations

from xpict.contracts.nodes import PictSpec, compress_pict, expand_pict
from xpict.contracts.spec import LabelPos, LabelSpec
from xpict.contracts.shorthand import (
    LABEL_DEFAULTS,
    compress_label,
    compress_pict_input,
    compress_shorthand,
    expand_label,
    expand_pict_input,
    expand_shorthand,
    map_dicts,
    map_tree,
)


def test_expand_shorthand_string_and_partial_dict():
    assert expand_shorthand("ethanol", LABEL_DEFAULTS) == {
        "text": "ethanol",
        "pos": "bottom",
    }
    assert expand_shorthand({"text": "ethanol"}, LABEL_DEFAULTS) == {
        "text": "ethanol",
        "pos": "bottom",
    }
    assert expand_shorthand(
        {"text": "ethanol", "pos": "top"}, LABEL_DEFAULTS
    ) == {"text": "ethanol", "pos": "top"}


def test_compress_shorthand_drops_defaults():
    assert compress_shorthand(
        {"text": "ethanol", "pos": "bottom"}, LABEL_DEFAULTS
    ) == "ethanol"
    assert compress_shorthand(
        {"text": "ethanol", "pos": "top"}, LABEL_DEFAULTS
    ) == {"text": "ethanol", "pos": "top"}
    assert compress_label({"text": "x", "pos": "bottom"}) == "x"


def test_map_tree_and_map_dicts():
    tree = {"a": 1, "b": [{"c": 2}, {"c": 3}]}
    doubled = map_tree(
        tree,
        lambda node, _path: node * 2 if isinstance(node, int) else node,
    )
    assert doubled == {"a": 2, "b": [{"c": 4}, {"c": 6}]}

    rewritten = map_dicts(
        tree,
        lambda d, path: {**d, "path": list(path)} if "c" in d else None,
    )
    assert rewritten["b"][0]["path"] == ["b", 0]
    assert rewritten["b"][1]["c"] == 3


def test_expand_pict_input_aliases_title_and_expands_label():
    raw = {
        "molecules": [
            {"smiles": "CCO", "title": "ethanol"},
            {"smiles": "c1ccccc1", "label": {"text": "benzene", "pos": "top"}},
        ]
    }
    out = expand_pict_input(raw)
    assert "title" not in out["molecules"][0]
    assert out["molecules"][0]["label"] == {"text": "ethanol", "pos": "bottom"}
    assert out["molecules"][1]["label"]["pos"] == "top"
    assert compress_pict_input(out)["molecules"][0]["label"] == "ethanol"
    assert compress_pict_input(out)["molecules"][1]["label"] == {
        "text": "benzene",
        "pos": "top",
    }


def test_pictspec_validates_to_expanded_label():
    doc = PictSpec.model_validate(
        {"molecules": [{"smiles": "CCO", "label": "ethanol"}]}
    )
    assert isinstance(doc.molecules[0].label, LabelSpec)
    assert doc.molecules[0].label.text == "ethanol"
    assert doc.molecules[0].label.pos is LabelPos.bottom

    doc2 = expand_pict(
        {"molecules": [{"smiles": "CCO", "label": {"text": "x", "pos": "left"}}]}
    )
    assert doc2.molecules[0].label.pos is LabelPos.left

    compressed = compress_pict(doc)
    # Nested dump: single-mol legacy lifts to a mol root
    assert compressed["type"] == "mol"
    assert compressed["layout"]["label"] == "ethanol"


def test_expand_label_none():
    assert expand_label(None) is None
    assert compress_label(None) is None

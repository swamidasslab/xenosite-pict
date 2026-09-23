"""Live DepictSpec — strict subset of future nested PictSpec."""

from __future__ import annotations

import pytest
from pydantic import ValidationError

from xpict import DepictSpec, MolSpec
from xpict.future import PictSpec


def test_live_mol_root():
    doc = DepictSpec.model_validate(
        {"type": "mol", "smiles": "CCO", "color": "#111"}
    )
    assert isinstance(doc.root, MolSpec)
    assert doc.root.smiles == "CCO"
    assert doc.mols()[0].color == "#111"


def test_live_star_labels():
    doc = DepictSpec.model_validate(
        {"type": "mol", "smiles": "*C", "star_labels": ["$R_1$", None]}
    )
    assert doc.root.star_labels == ["$R_1$", None]
    future = PictSpec.model_validate(doc.model_dump(mode="json"))
    assert future.root.star_labels == ["$R_1$", None]  # type: ignore[union-attr]


def test_live_group_children():
    doc = DepictSpec.model_validate(
        {
            "type": "group",
            "children": [
                {"type": "mol", "smiles": "CCO", "shade": {"atoms": [0.0, 0.2, 0.9]}},
                {"type": "mol", "cxsmiles": "*c1ccccc1Cl |$R1;;;;;$|"},
            ],
        }
    )
    assert len(doc.mols()) == 2
    assert doc.mols()[0].shade is not None
    assert doc.mols()[1].cxsmiles is not None


def test_molecules_key_rejected():
    with pytest.raises(ValidationError):
        DepictSpec.model_validate({"molecules": [{"smiles": "CCO"}]})


def test_kind_molecule_rejected():
    with pytest.raises(ValidationError):
        DepictSpec.model_validate(
            {"children": [{"kind": "molecule", "smiles": "CCO"}]}
        )


def test_live_doc_is_valid_future_pictspec():
    raw = {
        "type": "group",
        "children": [
            {
                "type": "mol",
                "smiles": "c1ccccc1O",
                "shade": {"atoms": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.85]},
            }
        ],
    }
    live = DepictSpec.model_validate(raw)
    future = PictSpec.model_validate(live.model_dump(mode="json"))
    assert future.root.type == "group"  # type: ignore[union-attr]


def test_live_align_to_string_or_object():
    from xpict.contracts.depict import AlignToSpec

    doc = DepictSpec.model_validate(
        {
            "type": "group",
            "align": True,
            "children": [
                {"type": "mol", "id": "ref", "smiles": "c1ccccc1"},
                {
                    "type": "mol",
                    "smiles": "Cc1ccccc1",
                    "align_to": {
                        "ref": "ref",
                        "atom_map": [[1, 0], [2, 1], [3, 2]],
                    },
                },
                {"type": "mol", "smiles": "CCO", "align_to": "ref"},
            ],
        }
    )
    kids = doc.mols()
    assert isinstance(kids[1].align_to, AlignToSpec)
    assert kids[1].align_to.ref == "ref"
    assert kids[2].align_to == "ref"
    future = PictSpec.model_validate(doc.model_dump(mode="json"))
    assert future.root.type == "group"  # type: ignore[union-attr]

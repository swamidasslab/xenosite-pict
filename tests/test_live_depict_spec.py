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


def test_live_group_children():
    doc = DepictSpec.model_validate(
        {
            "type": "group",
            "children": [
                {"type": "mol", "smiles": "CCO", "shade": {"atoms": [0.0, 0.2, 0.9]}},
                {"type": "mol", "cxsmiles": "*c1ccccc1Cl |$R_{1};;;;;$|"},
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


def test_mol_requires_structure():
    with pytest.raises(ValidationError):
        MolSpec.model_validate({"type": "mol"})

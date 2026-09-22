"""Live DepictSpec / MolSpec (shipped subset)."""

from __future__ import annotations

import pytest
from pydantic import ValidationError

from xpict import DepictSpec, MolSpec


def test_molspec_requires_structure():
    with pytest.raises(ValidationError):
        MolSpec.model_validate({})


def test_depict_batch_shape():
    doc = DepictSpec.model_validate(
        {
            "molecules": [
                {"smiles": "CCO", "mark_atoms": [2], "color": "#111"},
                {"source": "CCCO", "align_to": 0},
            ]
        }
    )
    assert len(doc.molecules) == 2
    assert doc.molecules[0].mark_atoms == [2]
    assert doc.molecules[1].source == "CCCO"
    assert doc.molecules[1].align_to == 0


def test_future_pictspec_still_importable():
    from xpict.future import PictSpec

    s = PictSpec.model_validate({"molecules": [{"smiles": "CCO"}]})
    assert s.molecules[0].smiles == "CCO"

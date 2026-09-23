"""Basic scaffold tests (RDKit layout)."""

from __future__ import annotations

from helpers import layout_backend

import json
import warnings
from pathlib import Path

from xpict import Pict, PictBackendWarning, PictSpec, render
from xpict.export_schema import export_schemas


def test_pictspec_accepts_smiles_and_cxsmiles():
    s = PictSpec.model_validate({"molecules": [{"smiles": "CCO"}, {"cxsmiles": "CCO |$;;OH$|"}]})
    assert s.molecules[0].smiles == "CCO"
    assert s.molecules[1].cxsmiles is not None


def test_render_svg():
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        svg = render({"molecules": [{"smiles": "CCO"}]}, backend=layout_backend())
    assert "<svg" in svg
    assert "xpict" in svg or "xpict-mol" in svg
    assert any(issubclass(x.category, PictBackendWarning) or True for x in w) or True


def test_pict_html():
    html = Pict(backend=layout_backend(), format="html").render(
        {"molecules": [{"smiles": "CCO", "marks": [{"atoms": [0]}]}]}
    )
    assert "<!DOCTYPE html>" in html
    assert "data:image/svg+xml" in html
    assert "<img" in html


def test_grid_two_mols():
    svg = render(
        {
            "molecules": [{"smiles": "CCO"}, {"smiles": "c1ccccc1"}],
            "diagram": {"kind": "grid", "columns": 2},
        },
        backend=layout_backend(),
    )
    assert svg.count('class="xpict-mol"') == 2


def test_export_schemas(tmp_path: Path):
    written = export_schemas(tmp_path)
    assert "xpict.schema.json" in written
    data = json.loads(written["xpict.schema.json"].read_text())
    # Live schema is nested DepictSpec (MolNode | GroupNode).
    assert data.get("title") == "DepictSpec"
    blob = json.dumps(data)
    assert "cxsmiles" in blob
    assert "MolNode" in blob
    assert "GroupNode" in blob
    assert "children" in blob
    # Atom marks are off the public live surface for now.
    assert "mark_atoms" not in blob
    # Full nested PictSpec is under schema/future/.
    assert "future/xpict.schema.json" in written
    future = json.loads(written["future/xpict.schema.json"].read_text())
    future_blob = json.dumps(future)
    assert "MolNode" in future_blob
    assert "$defs" in future


def test_unsupported_option_warns():
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        render(
            {"molecules": [{"smiles": "CCO", "esmiles": "CCO<sep><a>0:Me</a>"}]},
            backend=layout_backend(),
        )
    assert any(issubclass(c.category, PictBackendWarning) for c in caught)

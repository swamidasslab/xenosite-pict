"""Basic scaffold tests (RDKit layout)."""

from __future__ import annotations


import json
from pathlib import Path

from xpict import Pict, PictSpec, render
from xpict.export_schema import export_schemas


def test_pictspec_accepts_smiles_and_cxsmiles():
    s = PictSpec.model_validate({"molecules": [{"smiles": "CCO"}, {"cxsmiles": "CCO |$;;OH$|"}]})
    assert s.molecules[0].smiles == "CCO"
    assert s.molecules[1].cxsmiles is not None


def test_render_svg():
    svg = render({"molecules": [{"smiles": "CCO"}]})
    assert "<svg" in svg


def test_pict_html():
    html = Pict(format="html").render(
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
    )
    assert "<svg" in svg
    assert svg.count("<svg") >= 1


def test_export_schemas(tmp_path: Path):
    written = export_schemas(tmp_path)
    # Live schemas are Rust-owned (``make types``); this exporter only writes future.
    assert "xpict.schema.json" not in written
    assert "future/xpict.schema.json" in written
    future = json.loads(written["future/xpict.schema.json"].read_text())
    future_blob = json.dumps(future)
    assert "MolNode" in future_blob
    assert "$defs" in future

    # Committed live DepictSpec schema (schemars).
    live_path = Path(__file__).resolve().parents[1] / "schema" / "xpict.schema.json"
    data = json.loads(live_path.read_text())
    assert data.get("title") == "DepictSpec"
    blob = json.dumps(data)
    assert "cxsmiles" in blob
    assert "MolNode" in blob
    assert '"group"' in blob
    assert "children" in blob
    assert "star_labels" in blob
    # Atom marks / document rgroups are off the public live surface for now.
    assert "mark_atoms" not in blob
    assert "rgroups" not in blob


def test_esmiles_only_not_lifted_to_native():
    """Legacy ``esmiles`` is schema-valid but not a live MolNode source field."""
    import pytest

    with pytest.raises((ValueError, RuntimeError, Exception)):
        render({"molecules": [{"esmiles": "CCO<sep><a>0:Me</a>"}]})

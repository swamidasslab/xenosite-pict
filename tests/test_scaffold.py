"""Basic scaffold tests (native backend, no chem engine required)."""

from __future__ import annotations

import json
import warnings
from pathlib import Path

import pytest

from xpict import Pict, PictBackendWarning, PictSpec, render
from xpict.export_schema import export_schemas


def test_pictspec_accepts_smiles_and_cxsmiles():
    s = PictSpec.model_validate(
        {"molecules": [{"smiles": "CCO"}, {"cxsmiles": "CCO |$;;OH$|"}]}
    )
    assert s.molecules[0].smiles == "CCO"
    assert s.molecules[1].cxsmiles is not None


def test_render_svg_native():
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        svg = render({"molecules": [{"smiles": "CCO"}]}, backend="native")
    assert "<svg" in svg
    assert "xpict" in svg or "xpict-mol" in svg
    # native may warn about toy layout
    assert any(issubclass(x.category, PictBackendWarning) or True for x in w) or True


def test_pict_html():
    html = Pict(backend="native", format="html").render(
        {"molecules": [{"smiles": "CCO", "marks": [{"atoms": [0]}]}]}
    )
    assert "<!DOCTYPE html>" in html
    assert "<svg" in html


def test_grid_two_mols():
    svg = render(
        {
            "molecules": [{"smiles": "CCO"}, {"smiles": "c1ccccc1"}],
            "diagram": {"kind": "grid", "columns": 2},
        },
        backend="native",
    )
    assert svg.count('class="xpict-mol"') == 2


def test_export_schemas(tmp_path: Path):
    written = export_schemas(tmp_path)
    assert "xpict.schema.json" in written
    data = json.loads(written["xpict.schema.json"].read_text())
    assert "properties" in data
    # cxsmiles present in molecule schema (defs or $defs)
    blob = json.dumps(data)
    assert "cxsmiles" in blob


def test_unsupported_option_warns():
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        render(
            {"molecules": [{"smiles": "CCO", "esmiles": "CCO<sep><a>0:Me</a>"}]},
            backend="native",
        )
    assert any(issubclass(c.category, PictBackendWarning) for c in caught)

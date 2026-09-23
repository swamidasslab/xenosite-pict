"""EdgePlan schema + coord_gen processor (Python host)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

rdkit = pytest.importorskip("rdkit")

from xpict.contracts.edge import EdgePlan, EdgeResult
from xpict.edge_plan import build_align_plan, process_edge_plan, validate_edge_plan

SCHEMA = Path(__file__).resolve().parents[1] / "schema" / "edge-plan.schema.json"
RESULT_SCHEMA = Path(__file__).resolve().parents[1] / "schema" / "edge-result.schema.json"


def test_edge_plan_schema_committed():
    assert SCHEMA.is_file()
    assert RESULT_SCHEMA.is_file()
    data = json.loads(SCHEMA.read_text())
    assert data.get("title") == "EdgePlan"


def test_validate_rejects_duplicate_ids():
    plan = {
        "version": 1,
        "tasks": [
            {
                "type": "coord_gen",
                "roots": [
                    {
                        "id": "m_0",
                        "smiles": "CCO",
                        "template_for": [
                            {"id": "m_0", "smiles": "CCCO", "align": {}}
                        ],
                    }
                ],
            }
        ],
    }
    with pytest.raises(ValueError, match="duplicate"):
        validate_edge_plan(plan)


def test_process_atom_map_align():
    # Benzene ← toluene with explicit ring map (query methyl is 0).
    plan = build_align_plan(
        template_source="c1ccccc1",
        query_source="Cc1ccccc1",
        atom_map=[(1, 0), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5)],
    )
    result = process_edge_plan(plan)
    assert isinstance(result, EdgeResult)
    rows = result.results[0].molecules
    assert [r.id for r in rows] == ["m_0", "m_1"]
    assert rows[0].ok and rows[0].method == "free"
    assert rows[1].ok and rows[1].method == "atom_map"
    assert rows[1].molecule is not None
    assert rows[1].used_map is not None and len(rows[1].used_map) == 6


def test_process_mcs_align():
    plan = build_align_plan(
        template_source="c1ccc(O)cc1",
        query_source="O=C1C=CC(=O)C=C1",
        atom_map=None,
    )
    # build_align_plan with atom_map=None still sets AlignOpts() — MCS path
    result = process_edge_plan(plan)
    rows = {r.id: r for r in result.results[0].molecules}
    assert rows["m_0"].ok
    assert rows["m_1"].ok
    assert rows["m_1"].method == "mcs"


def test_align_failure_falls_back_to_free_layout():
    """Aliphatic vs quinone: align fails → unaligned coords; ok stays true."""
    plan = build_align_plan(
        template_source="O=C1C=CC(=O)C=C1",
        query_source="C1CCCCC1CCOCCCCCC",
        atom_map=None,
    )
    result = process_edge_plan(plan)
    rows = result.results[0].molecules
    assert len(rows) == 2
    assert rows[0].ok and rows[0].method == "free"
    assert rows[1].id == "m_1"
    assert rows[1].ok is True
    assert rows[1].method == "none"
    assert rows[1].molecule is not None
    assert rows[1].error and "fell back" in rows[1].error
    assert result.results[0].ok is True


def test_client_atom_map_option():
    from xpict import mol, render

    home = render(mol("c1ccccc1"))
    aligned = render(
        mol("Cc1ccccc1"),
        {
            "align_to": home,
            "atom_map": [(1, 0), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5)],
        },
    )
    hits = sum(
        1
        for a in aligned.coords
        if any(
            ((a.x - b.x) ** 2 + (a.y - b.y) ** 2) ** 0.5 < 0.5 for b in home.coords
        )
    )
    assert hits >= 6

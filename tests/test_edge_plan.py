"""EdgePlan schema + build helpers (host processor lives in Rust/JS)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from xpict.edge_plan import build_align_plan, process_edge_plan

SCHEMA = Path(__file__).resolve().parents[1] / "schema" / "edge-plan.schema.json"
RESULT_SCHEMA = Path(__file__).resolve().parents[1] / "schema" / "edge-result.schema.json"


def test_edge_plan_schema_committed():
    assert SCHEMA.is_file()
    assert RESULT_SCHEMA.is_file()
    data = json.loads(SCHEMA.read_text())
    assert data.get("title") == "EdgePlan"


def test_build_align_plan_shape():
    plan = build_align_plan(
        template_source="c1ccccc1",
        query_source="Cc1ccccc1",
        atom_map=[(1, 0), (2, 1)],
    )
    assert plan.version == 1
    assert len(plan.tasks) == 1
    root = plan.tasks[0].roots[0]
    assert root.smiles == "c1ccccc1"
    assert root.template_for[0].align is not None
    assert root.template_for[0].align.atom_map == [(1, 0), (2, 1)]


def test_process_edge_plan_removed():
    plan = build_align_plan(template_source="C", query_source="CC")
    with pytest.raises(NotImplementedError, match="removed"):
        process_edge_plan(plan)

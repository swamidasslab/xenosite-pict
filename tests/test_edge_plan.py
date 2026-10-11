"""EdgePlan schema (host processor lives in Rust)."""

from __future__ import annotations

import json
from pathlib import Path

SCHEMA = Path(__file__).resolve().parents[1] / "schema" / "edge-plan.schema.json"
RESULT_SCHEMA = Path(__file__).resolve().parents[1] / "schema" / "edge-result.schema.json"


def test_edge_plan_schema_committed():
    assert SCHEMA.is_file()
    assert RESULT_SCHEMA.is_file()
    data = json.loads(SCHEMA.read_text())
    assert data.get("title") == "EdgePlan"

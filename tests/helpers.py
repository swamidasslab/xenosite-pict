"""Shared test helpers (importable; not pytest fixtures)."""

from __future__ import annotations

import pytest


def layout_backend() -> str:
    """RDKit layout backend; skip the test when RDKit is not installed."""
    pytest.importorskip("rdkit")
    return "rdkit"

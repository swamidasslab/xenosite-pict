"""Pytest configuration — layout requires RDKit (see ``tests.helpers``)."""

from __future__ import annotations

# Re-export for fixtures / `from conftest import layout_backend` call sites.
from tests.helpers import layout_backend

__all__ = ["layout_backend"]

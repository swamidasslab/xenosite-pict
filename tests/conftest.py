"""Pytest configuration — layout requires RDKit (see ``helpers``)."""

from __future__ import annotations

# Re-export for call sites that import from conftest.
from helpers import layout_backend

__all__ = ["layout_backend"]

"""Shared test helpers (importable; not pytest fixtures)."""

from __future__ import annotations

import pytest


def layout_backend() -> str:
    """Legacy Pict layout backends were removed from the Python path."""
    # Raises XFailed — do not pass kwargs (``strict`` is mark-only).
    pytest.xfail(
        "legacy Pict layout backends removed; native CoordGen Pict path pending"
    )

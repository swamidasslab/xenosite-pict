"""Warnings for partial backend support."""

from __future__ import annotations


class PictBackendWarning(UserWarning):
    """Raised when a layout backend ignores or only partially supports an option."""

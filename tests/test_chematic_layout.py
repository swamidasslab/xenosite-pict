"""Experimental chematic_layout host flag (not DepictSpec schema)."""

from __future__ import annotations

import os

import pytest

from xpict import mol, render
from xpict import _native
from xpict.client import MolRenderOptions


@pytest.mark.skipif(
    not getattr(_native, "HAS_CHEMATIC_LAYOUT", False),
    reason="chematic-layout not built",
)
def test_chematic_layout_flag_paints_aspirin():
    out = render(mol("CC(=O)Oc1ccccc1C(=O)O"), MolRenderOptions(chematic_layout=True))
    assert out.width > 10 and out.height > 10
    assert len(out.coords) == 13
    assert "<svg" in out.to_svg()


@pytest.mark.skipif(
    not getattr(_native, "HAS_CHEMATIC_LAYOUT", False),
    reason="chematic-layout not built",
)
def test_chematic_layout_rejects_align():
    template = render(mol("c1ccccc1"))
    with pytest.raises(ValueError, match="free-layout only"):
        render(
            mol("c1ccccc1O"),
            MolRenderOptions(chematic_layout=True, align_to=template),
        )


@pytest.mark.skipif(
    not getattr(_native, "HAS_CHEMATIC_LAYOUT", False),
    reason="chematic-layout not built",
)
def test_chematic_layout_env_flag(monkeypatch):
    monkeypatch.setenv("XPICT_CHEMATIC_LAYOUT", "1")
    out = render(mol("CCO"))
    assert out.width > 10
    monkeypatch.delenv("XPICT_CHEMATIC_LAYOUT", raising=False)

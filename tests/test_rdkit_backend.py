"""RDKit layout backend smoke tests (optional extra)."""

from __future__ import annotations

import pytest

from xpict import Pict, render
from xpict.backends.base import get_backend


rdkit = pytest.importorskip("rdkit")


def test_rdkit_backend_registered():
    b = get_backend("rdkit")
    assert b.name == "rdkit"


def test_rdkit_layouts_ethanol():
    lay = Pict(backend="rdkit").layout({"molecules": [{"smiles": "CCO"}]}).molecules[0]
    assert lay.backend == "rdkit"
    assert len(lay.atoms) == 3
    assert len(lay.bonds) == 2
    oxy = next(a for a in lay.atoms if a.element == "O")
    assert oxy.label in {"OH", "O"}


def test_rdkit_default_when_installed():
    from xpict.api import _resolve_backend_name

    assert _resolve_backend_name(None) == "rdkit"


def test_rdkit_benzene_svg():
    svg = render({"molecules": [{"smiles": "c1ccccc1"}]}, backend="rdkit")
    assert "bond-skeleton" in svg or "bond-" in svg

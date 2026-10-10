"""Single-mol client surface after Python RDKit layout removal."""

from __future__ import annotations

import pytest

from xpict import Mol, mol, render


def test_mol_constructs():
    m = mol("CCO")
    assert isinstance(m, Mol)
    assert m.source == "CCO"


def test_render_awaits_native_bridge():
    with pytest.raises(NotImplementedError, match="removed"):
        render(mol("CCO"))


@pytest.mark.xfail(
    reason="Python host coord_gen removed; awaiting native CoordGen SMILES bridge",
    strict=True,
    raises=NotImplementedError,
)
def test_ethanol_svg_pending_bridge():
    render(mol("CCO")).to_svg()

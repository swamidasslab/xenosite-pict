"""Notebook HTML packing — multi-mol rows bottom-align captions."""

from __future__ import annotations

from xpict import mol
from xpict.display import render_depict_html


def test_multi_mol_html_uses_flex_end():
    """Names line up when SVGs bottom-align (align-items:flex-end)."""
    fig = mol("CCO").with_label("EtOH") + mol("C").with_label("methane")
    html = render_depict_html(fig.to_spec())
    assert "align-items:flex-end" in html
    assert "display:flex" in html


def test_single_mol_html_skips_flex_row():
    html = render_depict_html(mol("CCO").with_label("EtOH").to_spec())
    # Lone cell is inline-block, not a flex row.
    assert "align-items:flex-end" not in html
    assert "display:inline-block" in html

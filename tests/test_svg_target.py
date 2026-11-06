"""Host SVG medium defaults: PNG/PDF → print when unset."""

from __future__ import annotations

import pytest

from xpict.api import Pict
from xpict.svg_target import apply_output_svg_default


def test_apply_output_svg_default_png_injects_print() -> None:
    live = {"type": "mol", "smiles": "CCO"}
    out = apply_output_svg_default(live, "png")
    assert out["svg"] == "print"
    assert live.get("svg") is None  # input not mutated


def test_apply_output_svg_default_respects_author_svg() -> None:
    live = {"type": "mol", "smiles": "CCO", "svg": "web"}
    assert apply_output_svg_default(live, "pdf")["svg"] == "web"


def test_apply_output_svg_default_svg_format_stays_unset() -> None:
    live = {"type": "mol", "smiles": "CCO"}
    assert "svg" not in apply_output_svg_default(live, "svg")


def test_pict_png_raises_after_print_stamp() -> None:
    with pytest.raises(ValueError, match="print"):
        Pict(format="png").render({"type": "mol", "smiles": "CCO"})

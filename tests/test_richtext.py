"""Lightweight rich-text markup → Unicode spans → glyph shapes."""

from __future__ import annotations

import re

from xpict import render
from xpict.draw.glyphs import compile_text_path_d, compile_text_shapes
from xpict.draw.richtext import StyledText, TextRun, plain_text


def test_greek_and_symbol_macros():
    assert plain_text(r"\alpha-\beta") == "α-β"
    assert plain_text(r"$\alpha$-D-glucose") == "α-D-glucose"
    assert plain_text(r"T = 25\degree C") == "T = 25° C"
    assert plain_text(r"\Delta\Delta G") == "ΔΔ G"
    assert plain_text(r"\Delta\DeltaG") == "ΔΔG"
    assert plain_text(r"A \pm B \rightarrow C") == "A ± B → C"


def test_markup_yields_unicode_spans():
    styled = StyledText.from_markup(r"**bold** and *italic*")
    assert styled.runs == (
        TextRun("bold", bold=True),
        TextRun(" and "),
        TextRun("italic", italic=True),
    )
    styled = StyledText.from_markup(r"\textbf{\alpha} \textit{mode}")
    assert styled.runs[0] == TextRun("α", bold=True)
    assert any(r.text == "mode" and r.italic for r in styled.runs)


def test_compile_shapes_shared_engine():
    styled = StyledText.from_markup(r"$\alpha$-**EtOH**")
    geom = compile_text_shapes(styled, 50.0, 40.0, font_size=12.0)
    assert geom is not None and not geom.is_empty
    d = compile_text_path_d(styled, 50.0, 40.0, font_size=12.0)
    assert d and d.startswith("M ")
    # Same engine accepts markup strings directly.
    assert compile_text_path_d(r"\beta", 0.0, 0.0) is not None


def test_svg_emits_glyph_paths_not_text():
    svg = render(
        {"molecules": [{"smiles": "CCO", "label": r"$\alpha$-**EtOH**"}]},
        backend="native",
    )
    assert "<text" not in svg
    assert "<tspan" not in svg
    m = re.search(r'<path[^>]*class="mol-label"[^>]*>', svg)
    assert m is not None
    assert 'data-text="α-EtOH"' in m.group(0)
    assert "\\alpha" not in svg


def test_edge_label_markup_as_shapes():
    svg = render(
        {
            "molecules": [
                {"id": "a", "smiles": "CCO"},
                {"id": "b", "smiles": "CC=O"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [{"source": "a", "target": "b", "label": r"\Delta E"}],
            },
        },
        backend="native",
    )
    assert 'data-text="ΔE"' in svg or 'data-text="Δ E"' in svg
    assert "\\Delta" not in svg
    assert "<tspan" not in svg

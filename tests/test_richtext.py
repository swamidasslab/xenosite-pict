"""Lightweight rich-text markup for molecule / edge labels."""

from __future__ import annotations

from xenosite.pict import render
from xenosite.pict.draw.richtext import TextRun, plain_text, parse_richtext


def test_greek_and_symbol_macros():
    assert plain_text(r"\alpha-\beta") == "α-β"
    assert plain_text(r"$\alpha$-D-glucose") == "α-D-glucose"
    assert plain_text(r"T = 25\degree C") == "T = 25° C"
    assert plain_text(r"\Delta\Delta G") == "ΔΔ G"
    assert plain_text(r"\Delta\DeltaG") == "ΔΔG"
    assert plain_text(r"A \pm B \rightarrow C") == "A ± B → C"


def test_markdown_and_latex_emphasis():
    runs = parse_richtext(r"**bold** and *italic*")
    assert [(r.text, r.bold, r.italic) for r in runs] == [
        ("bold", True, False),
        (" and ", False, False),
        ("italic", False, True),
    ]
    runs = parse_richtext(r"\textbf{\alpha} \textit{mode}")
    assert runs[0] == TextRun("α", bold=True)
    assert any(r.text == "mode" and r.italic and not r.bold for r in runs)


def test_nested_style_and_symbol():
    runs = parse_richtext(r"\textbf{*β*-anomer}")
    assert plain_text(r"\textbf{*β*-anomer}") == "β-anomer"
    assert any(r.bold and r.italic and "β" in r.text for r in runs)


def test_svg_emits_tspans_for_styles():
    svg = render(
        {
            "molecules": [
                {
                    "smiles": "CCO",
                    "label": r"$\alpha$-**ethanol**",
                }
            ]
        },
        backend="native",
    )
    assert "α" in svg
    assert 'font-weight="bold"' in svg
    assert "ethanol" in svg
    assert "\\alpha" not in svg
    assert "**" not in svg


def test_edge_label_markup():
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
    assert "Δ" in svg
    assert "\\Delta" not in svg

"""Single-mol client parity with JS / Rust ``mol`` / ``render`` / ``to_svg``."""

from __future__ import annotations

import pytest

rdkit = pytest.importorskip("rdkit")

from xpict import Mol, MolRenderOptions, depict, mol, render, to_svg


def test_ethanol_svg():
    m = mol("CCO")
    rendered = render(m)
    svg = rendered.to_svg()
    assert "<svg" in svg
    assert "viewBox=" in svg
    assert "<path" in svg or "<circle" in svg
    assert rendered.width > 0 and rendered.height > 0
    assert len(rendered.molecule["atoms"]) == 3
    assert len(rendered.molecule["bonds"]) == 2
    assert rendered.frame_molblock


def test_to_svg_from_scene():
    r = mol("c1ccccc1").render()
    svg = to_svg(r.scene)
    assert "<svg" in svg


def test_cx_star_labels_by_index():
    r = render(mol("*C* |$;;R2;$|"))
    labels = [a.label for a in r.coords if a.label]
    assert "R2" in labels


def test_star_labels_override():
    r = render(mol("*C"), MolRenderOptions(star_labels=["R1"]))
    assert any(a.label == "R1" for a in r.coords)


def test_align_to_mol_overlay():
    benzene = mol("c1ccccc1")
    home = render(benzene)
    aligned = render(mol("Cc1ccccc1"), {"align_to": benzene})
    hits = 0
    for a in aligned.coords:
        if any(
            ((a.x - b.x) ** 2 + (a.y - b.y) ** 2) ** 0.5 < 0.5 for b in home.coords
        ):
            hits += 1
    assert hits >= 6, f"align_to mol: expected ≥6 overlay hits, got {hits}"


def test_align_to_rendered_overlay():
    home = render(mol("c1ccccc1"))
    aligned = render(mol("Oc1ccccc1"), MolRenderOptions(align_to=home))
    hits = sum(
        1
        for a in aligned.coords
        if any(
            ((a.x - b.x) ** 2 + (a.y - b.y) ** 2) ** 0.5 < 0.5 for b in home.coords
        )
    )
    assert hits >= 6, f"align_to rendered: expected ≥6 overlay hits, got {hits}"


def test_phenol_quinone_mcs():
    phenol = mol("c1ccc(O)cc1")
    ph = render(phenol)
    q = render(mol("O=C1C=CC(=O)C=C1"), {"align_to": phenol})
    hits = sum(
        1
        for a in q.coords
        if any(((a.x - b.x) ** 2 + (a.y - b.y) ** 2) ** 0.5 < 0.5 for b in ph.coords)
    )
    assert hits >= 6, f"quinone→phenol: expected ≥6 hits, got {hits}"


def test_multi_query_leaves_template_frame():
    tmpl = mol("c1ccc(O)cc1")
    render(tmpl)
    before = tmpl.frame_molblock
    assert before
    for smi in ("O=C1C=CC(=O)C=C1", "Cc1ccc(O)cc1", "Clc1ccc(O)cc1"):
        render(mol(smi), {"align_to": tmpl})
        assert tmpl.frame_molblock == before


def test_document_render_still_works():
    svg = render({"type": "mol", "smiles": "CCO"})
    assert isinstance(svg, str) and "<svg" in svg
    svg2 = depict({"type": "mol", "smiles": "CCO"})
    assert "<svg" in svg2


def test_color_option():
    r = render(mol("CCO"), {"color": "#0b6e4f"})
    assert r.molecule.get("color") == "#0b6e4f"

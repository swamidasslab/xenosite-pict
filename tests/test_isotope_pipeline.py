"""Isotopes on input mols must survive layout → MoleculeIn → SVG.

Covers the single-mol client pipeline (``mol`` / ``render`` / ``layout_with_rdkit``)
and free-layout pose molblocks. Legacy Pict layout coverage lives in
``test_chem_edge_cases``.
"""

from __future__ import annotations

import re

import pytest

rdkit = pytest.importorskip("rdkit")
from rdkit import Chem

from xpict import mol, render, to_svg
from xpict.client import layout_with_rdkit


def _data_texts(svg: str) -> list[str]:
    return re.findall(r'data-text="([^"]*)"', svg)


def _normalize(text: str) -> str:
    return (
        text.replace("²", "2")
        .replace("³", "3")
        .replace("¹", "1")
        .replace("⁰", "0")
        .replace("⁴", "4")
        .replace("⁵", "5")
        .replace("⁶", "6")
        .replace("⁷", "7")
        .replace("⁸", "8")
        .replace("⁹", "9")
    )


@pytest.mark.parametrize(
    "smiles,mass,label_substr",
    [
        ("[2H]C", 2, "2H"),
        ("C[13CH3]", 13, "13CH3"),
        ("[13CH4]", 13, "13CH4"),
        ("C[2H]", 2, "2H"),
        ("[14CH3]O", 14, "14CH3"),
    ],
)
def test_isotope_survives_client_layout_and_svg(
    smiles: str, mass: int, label_substr: str
) -> None:
    molecule, pose_mb, meta = layout_with_rdkit(smiles)
    assert meta["method"] == "free"
    labels = [a.get("label") or "" for a in molecule["atoms"]]
    assert any(label_substr in lab for lab in labels), labels

    # Pose molblock keeps the mass number (V2000 M ISO).
    assert "M  ISO" in pose_mb
    assert str(mass) in pose_mb
    rmol = Chem.MolFromMolBlock(pose_mb, sanitize=True, removeHs=False)
    assert rmol is not None
    assert any(int(a.GetIsotope()) == mass for a in rmol.GetAtoms())

    rendered = render(mol(smiles))
    texts = [_normalize(t) for t in _data_texts(rendered.to_svg())]
    assert any(label_substr in t or str(mass) in t for t in texts), texts


def test_isotope_survives_align_to_pipeline() -> None:
    """Deuterated query aligned onto a related scaffold still paints 2H."""
    home = render(mol("CCO"))
    aligned = render(mol("[2H]CCO"), {"align_to": home})
    labels = [a.get("label") or "" for a in aligned.molecule["atoms"]]
    assert any("2H" in lab for lab in labels), labels
    assert "M  ISO" in aligned.frame_molblock
    rmol = Chem.MolFromMolBlock(
        aligned.frame_molblock, sanitize=True, removeHs=False
    )
    assert rmol is not None
    assert any(int(a.GetIsotope()) == 2 for a in rmol.GetAtoms())
    texts = [_normalize(t) for t in _data_texts(aligned.to_svg())]
    assert any("2H" in t or "2" in t for t in texts), texts


def test_isotope_carbon_gets_mass_prefix_label() -> None:
    """Isotopic carbon is not suppressed like plain C."""
    molecule, _, _ = layout_with_rdkit("C[13CH3]")
    carbon_labels = [
        a.get("label")
        for a in molecule["atoms"]
        if a.get("element") == "C" and a.get("label")
    ]
    assert any(lab and "13" in lab for lab in carbon_labels), molecule["atoms"]


def test_to_svg_scene_keeps_isotope_label() -> None:
    r = mol("[2H]O").render()
    svg = to_svg(r.scene)
    texts = [_normalize(t) for t in _data_texts(svg)]
    assert any("2H" in t for t in texts), texts

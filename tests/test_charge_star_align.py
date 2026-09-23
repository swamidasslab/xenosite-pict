"""Formal charge, radicals, stars/R-group names, and MCS alignment."""

from __future__ import annotations

from helpers import layout_backend

import math
import warnings

import pytest

from xpict import Pict, PictBackendWarning, render
from xpict.align import _mcs_mapping, align_layouts


def _backend() -> str:
    return layout_backend()


def test_rdkit_parses_star_and_charge():
    pytest.importorskip("rdkit")
    from rdkit import Chem

    mol = Chem.MolFromSmiles("*C")
    assert mol.GetAtomWithIdx(0).GetAtomicNum() == 0
    mol2 = Chem.MolFromSmiles("[NH4+]")
    assert mol2.GetAtomWithIdx(0).GetFormalCharge() == 1
    mol3 = Chem.MolFromSmiles("[O-]")
    assert mol3.GetAtomWithIdx(0).GetFormalCharge() == -1
    mol4 = Chem.MolFromSmiles("[CH3]")
    assert mol4.GetAtomWithIdx(0).GetNumRadicalElectrons() == 1


def test_charge_label_in_svg():
    svg = render({"molecules": [{"smiles": "[NH4+]"}]}, backend=layout_backend())
    assert "N" in svg
    assert "+" in svg or "⁺" in svg or "＋" in svg


def test_radical_dot_in_svg():
    svg = render({"molecules": [{"smiles": "[CH3]"}]}, backend=layout_backend())
    assert "radical" in svg


def test_star_label():
    lay = Pict(backend=layout_backend()).layout({"molecules": [{"smiles": "*C"}]}).molecules[0]
    star = next(a for a in lay.atoms if a.element == "*")
    assert star.label == "*"
    svg = render({"molecules": [{"smiles": "*C"}]}, backend=layout_backend())
    assert ">" in svg and "*" in svg


def test_star_name_from_cxsmiles():
    lay = Pict(backend=layout_backend()).layout({"molecules": [{"cxsmiles": "*C |$R1;$|"}]}).molecules[0]
    star = next(a for a in lay.atoms if a.element == "*")
    assert star.label == "R1"
    svg = render({"molecules": [{"cxsmiles": "*C |$R1;$|"}]}, backend=layout_backend())
    assert "R1" in svg


@pytest.mark.skip(reason="indigo not in MVP")
def test_indigo_star_and_charge():
    lay = Pict(backend="indigo").layout({"molecules": [{"smiles": "*C"}]}).molecules[0]
    assert any(a.element == "*" and a.label for a in lay.atoms)
    lay2 = Pict(backend="indigo").layout({"molecules": [{"smiles": "[NH4+]"}]}).molecules[0]
    assert lay2.atoms[0].charge == 1
    svg = render({"molecules": [{"smiles": "C[O]"}]}, backend="indigo")
    assert "radical" in svg


@pytest.mark.skip(reason="indigo not in MVP")
def test_indigo_cx_star_name():
    lay = (
        Pict(backend="indigo")
        .layout({"molecules": [{"cxsmiles": "*C1ccccc1 |$R1;;;;;$|"}]})
        .molecules[0]
    )
    star = next(a for a in lay.atoms if a.element == "*")
    assert star.label == "R1"


def test_align_mcs_maps_phenol_scaffold():
    backend = _backend()
    pict = Pict(backend=backend)
    ph = pict.layout({"molecules": [{"smiles": "c1ccc(cc1)O"}]}).molecules[0]
    an = pict.layout({"molecules": [{"smiles": "COc1ccccc1"}]}).molecules[0]
    mapping = _mcs_mapping(ph, an)
    assert mapping is not None
    assert len(mapping) >= 6  # phenyl carbons at least


def test_align_layouts_transforms_coords():
    backend = _backend()
    pict = Pict(backend=backend)
    a = pict.layout({"molecules": [{"smiles": "c1ccc(cc1)O"}]}).molecules[0]
    b = pict.layout({"molecules": [{"smiles": "COc1ccccc1"}]}).molecules[0]
    # Flip B so alignment must rotate/translate.
    flipped = b.model_copy(
        update={"atoms": [at.model_copy(update={"x": -at.x, "y": -at.y + 5.0}) for at in b.atoms]}
    )
    aligned = align_layouts([a, flipped], enabled=True)
    assert len(aligned) == 2

    # Ring centroids should be closer after alignment than raw flipped.
    def centroid(lay):
        xs = [at.x for at in lay.atoms]
        ys = [at.y for at in lay.atoms]
        return sum(xs) / len(xs), sum(ys) / len(ys)

    ca = centroid(aligned[0])
    cb = centroid(aligned[1])
    dist = math.hypot(ca[0] - cb[0], ca[1] - cb[1])
    assert dist < 2.0


def test_render_align_flag_no_warning():
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        render(
            {
                "molecules": [
                    {"smiles": "c1ccc(cc1)O"},
                    {"smiles": "COc1ccccc1"},
                ],
                "diagram": {"kind": "grid", "columns": 2, "align": True},
            },
            backend=_backend(),
        )
    assert not any(
        issubclass(c.category, PictBackendWarning) and "not implemented" in str(c.message)
        for c in caught
    )

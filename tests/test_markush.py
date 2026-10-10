"""Markush rgroups, named rings, and ring attachments."""

from __future__ import annotations


import pytest

from xpict import Pict, render
from xpict.future.spec import MoleculeSpec
from xpict.draw.drawable import display_text
from xpict.draw.markush import (
    apply_rgroup_texts,
    resolve_rgroups,
    resolve_ring_atoms,
    ring_attachment_annotations,
    rtable_groups,
    star_atom_indices,
)


def test_rgroups_list_assigns_stars_in_order():
    lay = Pict().layout({"molecules": [{"smiles": "*C*"}]})[0]
    stars = star_atom_indices(lay)
    assert len(stars) == 2
    ov = resolve_rgroups(lay, ["R¹", None])
    assert ov[stars[0]] == "R¹"
    assert ov[stars[1]] is None


def test_rgroups_dict_by_star_ordinal():
    lay = Pict().layout({"molecules": [{"smiles": "*CC*"}]})[0]
    ov = resolve_rgroups(lay, {"1": "R2", "0": "R1"})
    stars = star_atom_indices(lay)
    assert ov[stars[0]] == "R1"
    assert ov[stars[1]] == "R2"


@pytest.mark.xfail(
    reason="legacy rgroups not on live MolNode; use star_labels",
    strict=True,
)
def test_rgroups_render_label_on_star():
    svg = render(
        {
            "molecules": [
                {"smiles": "*C", "rgroups": ["R¹"]},
            ]
        },
    )
    assert "R¹" in svg or 'data-text="R¹"' in svg or "R" in svg


def test_rings_and_ring_attachment_schema():
    mol = MoleculeSpec.model_validate(
        {
            "smiles": "c1ccccc1",
            "rings": {"A": [0, 1, 2, 3, 4, 5]},
            "ring_attachments": [{"ring": "A", "label": "R"}],
        }
    )
    assert mol.rings["A"] == [0, 1, 2, 3, 4, 5]
    assert mol.ring_attachments[0].label == "R"
    atoms = resolve_ring_atoms("A", mol.rings)
    assert atoms == [0, 1, 2, 3, 4, 5]
    anns = ring_attachment_annotations(mol)
    assert len(anns) == 1
    assert anns[0].ring == [0, 1, 2, 3, 4, 5]
    assert anns[0].label == "R"


def test_duplicate_ring_attachment_labels_ok():
    mol = MoleculeSpec.model_validate(
        {
            "smiles": "c1ccccc1",
            "rings": {"A": [0, 1, 2, 3, 4, 5]},
            "ring_attachments": [
                {"ring": "A", "label": "R"},
                {"ring": [0, 1, 2, 3, 4, 5], "label": "R"},
            ],
        }
    )
    assert len(mol.ring_attachments) == 2
    assert rtable_groups(mol) == ["R"]


def test_ids_rings_name_collision_rejected():
    with pytest.raises(ValueError, match="unique"):
        MoleculeSpec.model_validate(
            {
                "smiles": "c1ccccc1",
                "ids": {"A": 0},
                "rings": {"A": [0, 1, 2, 3, 4, 5]},
            }
        )


def test_unknown_ring_name_rejected():
    with pytest.raises(ValueError, match="unknown ring"):
        MoleculeSpec.model_validate(
            {
                "smiles": "c1ccccc1",
                "ring_attachments": [{"ring": "A", "label": "R"}],
            }
        )


def test_rtable_shorthand_rows():
    mol = MoleculeSpec.model_validate(
        {
            "smiles": "*C",
            "rgroups": ["R1"],
            "rtable": [["Me"], ["Et"]],
        }
    )
    assert mol.rtable is not None
    assert mol.rtable.rows == [["Me"], ["Et"]]
    assert rtable_groups(mol) == ["R1"]


@pytest.mark.xfail(
    reason="legacy annot callouts not on live DepictSpec",
    strict=True,
)
def test_ring_attachment_draws_callout():
    svg = render(
        {
            "molecules": [
                {
                    "smiles": "c1ccccc1",
                    "rings": {"A": [0, 1, 2, 3, 4, 5]},
                    "ring_attachments": [{"ring": "A", "label": "R", "prefer": "right"}],
                }
            ]
        },
    )
    assert "annot-label" in svg or "annot-arrow" in svg


def test_apply_rgroup_texts_overrides_display():
    lay = Pict().layout({"molecules": [{"smiles": "*C"}]})[0]
    mol = MoleculeSpec.model_validate({"smiles": "*C", "rgroups": ["X"]})
    texts = apply_rgroup_texts(lay, mol, [display_text(a) for a in lay.atoms])
    star_i = next(i for i, a in enumerate(lay.atoms) if a.element == "*")
    assert texts[star_i] == "X"


def test_star_labels_wins_over_rgroups():
    lay = Pict().layout({"molecules": [{"smiles": "*C"}]})[0]
    mol = MoleculeSpec.model_validate(
        {"smiles": "*C", "rgroups": ["X"], "star_labels": ["Y"]}
    )
    texts = apply_rgroup_texts(lay, mol, [display_text(a) for a in lay.atoms])
    star_i = next(i for i, a in enumerate(lay.atoms) if a.element == "*")
    assert texts[star_i] == "Y"

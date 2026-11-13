"""Align pose carry-over: coords / molblock / visible / align_from."""

from __future__ import annotations

import json

import pytest

from xpict import align_from, depict, mol, render
from xpict.contracts.depict import AlignToSpec
from xpict.spec import MolRenderOptions


def test_b1_partial_coords_seed() -> None:
    rows = depict(
        {
            "type": "mol",
            "smiles": "CCO",
            "align_to": {
                "coords": {"x": [0.0, 1.5], "y": [0.0, 0.0], "atom": [0, 1]},
            },
        }
    )
    assert len(rows) == 1
    assert len(rows[0].coords) == 3


def test_b2_align_from_atom_map_remaps_to_query_indices() -> None:
    prior = depict({"type": "mol", "smiles": "CCO"})[0]
    frame = json.loads(prior.frame_molblock)
    by_t = {int(a["index"]): a for a in frame["atoms"]}
    spec = prior.align_from(atom_map=[(0, 0), (1, 1), (2, 2)])
    assert isinstance(spec, AlignToSpec)
    assert spec.molblock is None
    assert spec.coords is not None
    assert list(spec.coords.atom) == [0, 1, 2]
    for i, (q, t) in enumerate([(0, 0), (1, 1), (2, 2)]):
        assert spec.coords.atom[i] == q
        assert spec.coords.x[i] == pytest.approx(float(by_t[t]["x"]))
        assert spec.coords.y[i] == pytest.approx(float(by_t[t]["y"]))


def test_b3_visible_false_omits_template() -> None:
    rows = depict(
        {
            "type": "group",
            "children": [
                {"type": "mol", "id": "t", "smiles": "c1ccccc1O", "visible": False},
                {"type": "mol", "smiles": "Cc1ccccc1O", "align_to": "t"},
            ],
        }
    )
    assert len(rows) == 1


def test_c1_molblock_mcs() -> None:
    prior = depict({"type": "mol", "smiles": "CCO"})[0]
    mb = prior.align_from(timeout_ms=50).molblock
    assert mb and "V2000" in mb
    rows = depict(
        {
            "type": "mol",
            "smiles": "CCO",
            "align_to": {"molblock": mb, "timeout_ms": 50},
        }
    )
    assert len(rows) == 1


def test_c2_align_from_default_molblock() -> None:
    a = depict({"type": "mol", "smiles": "c1ccccc1O"})[0]
    spec = a.align_from()
    assert spec.molblock and "V2000" in spec.molblock
    assert spec.coords is None


def test_legacy_rendered_wrap_carries_pose() -> None:
    ra = render(mol("CCO"))
    spec = mol("CCCO").to_spec(MolRenderOptions(align_to=ra))
    assert spec.root.type == "mol"
    assert isinstance(spec.root.align_to, AlignToSpec)
    assert spec.root.align_to.molblock
    rb = render(mol("CCCO"), MolRenderOptions(align_to=ra))
    assert len(rb.coords) == 4


def test_legacy_mol_with_frame_carries_pose() -> None:
    tmpl = mol("CCO")
    render(tmpl)
    assert tmpl.frame_molblock
    spec = mol("CCCO").to_spec(MolRenderOptions(align_to=tmpl))
    assert spec.root.type == "mol"
    assert isinstance(spec.root.align_to, AlignToSpec)
    assert spec.root.align_to.molblock


def test_align_from_rows_id() -> None:
    rows = depict(
        {
            "type": "group",
            "children": [
                {"type": "mol", "id": "t", "smiles": "c1ccccc1"},
                {"type": "mol", "id": "q", "smiles": "Cc1ccccc1", "align_to": "t"},
            ],
        }
    )
    af = align_from(rows, "t", timeout_ms=20)
    assert af.molblock
    assert af.timeout_ms == 20


def test_ref_plus_coords_rejected() -> None:
    with pytest.raises(Exception):
        depict(
            {
                "type": "mol",
                "smiles": "CCO",
                "align_to": {
                    "ref": "t",
                    "coords": {"x": [0.0], "y": [0.0], "atom": [0]},
                },
            }
        )

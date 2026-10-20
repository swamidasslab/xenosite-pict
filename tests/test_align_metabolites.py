"""Parent–metabolite and related-scaffold atom maps (native house align)."""

from __future__ import annotations

import json

import pytest
from xenosite.forest import ForestMol, hydroxylation

from xpict import mol
from xpict.contracts.depict import DepictSpec
from xpict.display import render_depict_mols

MORPHINE = "CN1CCC23c4c5ccc(O)c4OC2C(O)C=CC3C1C5"
CODEINE = "CN1CCC23c4c5ccc(OC)c4OC2C(O)C=CC3C1C5"
CHOLESTEROL = "CC(C)CCCC(C)C1CCC2C3CC=C4CC(O)CCC4(C)C3CCC12C"
TESTOSTERONE = "CC12CCC3C(CCC4=CC(=O)CCC34C)C1CCC2O"


def _align(parent: str, query: str) -> tuple[dict, dict, list[tuple[int, int]]]:
    spec = mol(parent) + mol(query)
    dump = spec.model_dump(exclude_none=True)
    dump["debug_align"] = True
    rows = render_depict_mols(DepictSpec.model_validate(dump))
    assert len(rows) == 2
    ref = json.loads(rows[0].frame_molblock)
    qry = json.loads(rows[1].frame_molblock)
    dbg = rows[1].molecule.align_debug
    tags = [(int(q), int(r)) for q, r in (dbg.tags if dbg is not None else [])]
    return ref, qry, tags


def _el(frame: dict, idx: int) -> str:
    for atom in frame["atoms"]:
        if int(atom["index"]) == idx:
            return str(atom.get("element") or "")
    raise KeyError(idx)


def test_codeine_maps_onto_morphine_scaffold() -> None:
    ref, qry, tags = _align(MORPHINE, CODEINE)
    by_q = dict(tags)
    assert len(tags) == 21
    assert all(_el(qry, q) == _el(ref, r) for q, r in tags)
    unmatched = {int(a["index"]) for a in qry["atoms"]} - set(by_q)
    assert unmatched == {11}  # extra methoxy carbon
    assert by_q[10] == 10  # phenolic / methoxy oxygen
    assert by_q[12] == 11  # skip the extra methyl


def test_testosterone_does_not_map_oxygen_onto_cholesterol_carbon() -> None:
    ref, qry, tags = _align(CHOLESTEROL, TESTOSTERONE)
    assert tags
    assert all(_el(qry, q) == _el(ref, r) for q, r in tags)
    assert len(tags) == 20  # extra O unmatched; no O→C


@pytest.mark.parametrize(
    "parent",
    [
        "c1ccccc1O",
        "CC(=O)Nc1ccc(O)cc1",
        "Cn1cnc2n(C)c(=O)n(C)c(=O)c12",
    ],
)
def test_hydroxylation_metabolite_maps_parent_atoms(parent: str) -> None:
    emissions = hydroxylation().metabolize(ForestMol(parent))
    assert emissions, parent
    for em in emissions[:3]:
        prod = em.product_csmis()[0]
        ref, qry, tags = _align(parent, prod)
        assert len(tags) == len(ref["atoms"]), (parent, prod, tags)
        assert all(_el(qry, q) == _el(ref, r) for q, r in tags)

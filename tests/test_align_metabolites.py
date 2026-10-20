"""Parent–metabolite and related-scaffold atom maps (native house align)."""

from __future__ import annotations

import json

import pytest
from xenosite.forest import (
    ForestMol,
    dehydrogenation,
    epoxidation,
    epoxide_opening,
    hydroxylation,
    quinone_formation,
)

from xpict import mol
from xpict.contracts.depict import DepictSpec
from xpict.display import render_depict_mols

MORPHINE = "CN1CCC23c4c5ccc(O)c4OC2C(O)C=CC3C1C5"
CODEINE = "CN1CCC23c4c5ccc(OC)c4OC2C(O)C=CC3C1C5"
CHOLESTEROL = "CC(C)CCCC(C)C1CCC2C3CC=C4CC(O)CCC4(C)C3CCC12C"
TESTOSTERONE = "CC12CCC3C(CCC4=CC(=O)CCC34C)C1CCC2O"
ERYTHROMYCIN = (
    "CCC1OC(=O)C(C)C(OC2CC(C)(OC)C(O)C(C)O2)C(C)C(OC2OC(C)CC(N(C)C)C2O)"
    "C(C)(O)CC(C)C(=O)C(C)C(O)C1(C)O"
)
AZITHROMYCIN = (
    "CCC1CC(C)(O)C(O)C(C)N(C)CC(C)CC(C)(O)C(OC2OC(C)CC(N(C)C)C2O)C(C)C"
    "(OC2CC(C)(OC)C(O)C(C)O2)C(C)C(=O)O1"
)
HYDROQUINONE = "Oc1ccc(O)cc1"
BENZOQUINONE = "O=C1C=CC(=O)C=C1"
APAP = "CC(=O)Nc1ccc(O)cc1"
NAPQI = "CC(=O)N=C1C=CC(=O)C=C1"
BENZENE = "c1ccccc1"
BENZENE_OXIDE = "C1=CC2OC2C=C1"
PHENOL = "c1ccccc1O"


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


def _nme2(frame: dict) -> int:
    nbrs: dict[int, list[int]] = {}
    for bond in frame["bonds"]:
        u, v = int(bond["begin"]), int(bond["end"])
        nbrs.setdefault(u, []).append(v)
        nbrs.setdefault(v, []).append(u)
    deg = {i: len(nbrs.get(i, [])) for i in range(len(frame["atoms"]))}
    for atom in frame["atoms"]:
        i = int(atom["index"])
        if atom.get("element") != "N":
            continue
        terms = [
            j
            for j in nbrs.get(i, [])
            if _el(frame, j) == "C" and deg.get(j, 0) == 1
        ]
        if len(terms) >= 2:
            return i
    raise AssertionError("no NMe2")


def test_codeine_maps_onto_morphine_scaffold() -> None:
    ref, qry, tags = _align(MORPHINE, CODEINE)
    by_q = dict(tags)
    assert len(tags) == 21
    assert all(_el(qry, q) == _el(ref, r) for q, r in tags)
    unmatched = {int(a["index"]) for a in qry["atoms"]} - set(by_q)
    assert unmatched == {11}  # extra methoxy carbon
    assert by_q[10] == 10  # phenolic / methoxy oxygen
    assert by_q[12] == 11  # skip the extra methyl


def test_azithromycin_maps_onto_erythromycin_without_flipping_desosamine() -> None:
    ref, qry, tags = _align(ERYTHROMYCIN, AZITHROMYCIN)
    by_q = dict(tags)
    assert len(tags) >= 40, tags
    assert all(_el(qry, q) == _el(ref, r) for q, r in tags)
    q_n = _nme2(qry)
    r_n = _nme2(ref)
    assert by_q.get(q_n) == r_n, by_q
    ring_n = [
        int(a["index"])
        for a in qry["atoms"]
        if a.get("element") == "N" and int(a["index"]) != q_n
    ]
    assert ring_n and ring_n[0] not in by_q


def test_testosterone_does_not_map_oxygen_onto_cholesterol_carbon() -> None:
    ref, qry, tags = _align(CHOLESTEROL, TESTOSTERONE)
    assert tags
    assert all(_el(qry, q) == _el(ref, r) for q, r in tags)
    assert len(tags) == 20  # extra O unmatched; no O→C


def _assert_parent_maps(parent: str, prod: str) -> None:
    ref, qry, tags = _align(parent, prod)
    assert len(tags) == len(ref["atoms"]), (parent, prod, tags)
    assert all(_el(qry, q) == _el(ref, r) for q, r in tags)


def _remodel_products(rule, parent: str, *, limit: int = 4) -> list[str]:
    """Single-piece conjugate products that keep the parent heavy-atom count."""
    out: list[str] = []
    n_parent = len(_align(parent, parent)[0]["atoms"])
    for em in rule.metabolize(ForestMol(parent)):
        for prod in em.product_csmis():
            if not prod or "." in prod or prod in out:
                continue
            n_prod = len(_align(parent, prod)[1]["atoms"])
            if n_prod < n_parent - 1:
                continue
            out.append(prod)
            if len(out) >= limit:
                return out
    return out


@pytest.mark.parametrize(
    "parent",
    [
        PHENOL,
        APAP,
        "Cn1cnc2n(C)c(=O)n(C)c(=O)c12",
    ],
)
def test_hydroxylation_metabolite_maps_parent_atoms(parent: str) -> None:
    emissions = hydroxylation().metabolize(ForestMol(parent))
    assert emissions, parent
    for em in emissions[:3]:
        prod = em.product_csmis()[0]
        _assert_parent_maps(parent, prod)


def test_hydroquinone_maps_onto_benzoquinone() -> None:
    _assert_parent_maps(HYDROQUINONE, BENZOQUINONE)


def test_apap_maps_onto_napqi() -> None:
    _assert_parent_maps(APAP, NAPQI)


@pytest.mark.parametrize("parent", [HYDROQUINONE, APAP])
def test_quinone_formation_maps_parent_ring(parent: str) -> None:
    products = _remodel_products(quinone_formation(), parent)
    assert products, parent
    for prod in products:
        _assert_parent_maps(parent, prod)


def test_benzene_epoxide_maps_parent_carbons() -> None:
    products = _remodel_products(epoxidation(), BENZENE)
    assert products, BENZENE
    for prod in products:
        _assert_parent_maps(BENZENE, prod)


def test_benzene_oxide_opening_maps_onto_phenol() -> None:
    products = _remodel_products(epoxide_opening(), BENZENE_OXIDE)
    assert products, BENZENE_OXIDE
    for prod in products:
        _assert_parent_maps(BENZENE_OXIDE, prod)


def test_tetralin_dehydrogenation_maps_parent_atoms() -> None:
    parent = "C1CCc2ccccc2C1"
    products = _remodel_products(dehydrogenation(), parent)
    assert products, parent
    for prod in products:
        _assert_parent_maps(parent, prod)

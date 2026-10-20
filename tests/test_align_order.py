"""Alignment must not depend on SMILES atom order (native Rust align)."""

from __future__ import annotations

import json

import pytest

from xpict import mol
from xpict.depict_spec import depict


def _frames(query: str) -> tuple[dict, dict]:
    ref, out = depict(mol("CCCCCC") + mol(query))
    return json.loads(str(ref.frame())), json.loads(str(out.frame()))


def _chain_xy(frame: dict, ring: set[int]) -> list[tuple[float, float]]:
    return sorted(
        (round(a["x"], 1), round(a["y"], 1))
        for a in frame["atoms"]
        if a["index"] not in ring
    )


@pytest.mark.parametrize(
    ("query", "ring"),
    [
        ("C1CCCCC1CCCCC", {0, 1, 2, 3, 4}),  # ipso (5) kept with the chain
        ("CCCCCC1CCCCC1", {6, 7, 8, 9, 10}),  # ipso (5) kept with the chain
    ],
)
def test_hexane_aligns_onto_pentyl_chain(query: str, ring: set[int]) -> None:
    ref, out = _frames(query)
    hexane = sorted((round(a["x"], 1), round(a["y"], 1)) for a in ref["atoms"])
    got = [c for xy in _chain_xy(out, ring) for c in xy]
    want = [c for xy in hexane for c in xy]
    assert got == pytest.approx(want, abs=0.5)

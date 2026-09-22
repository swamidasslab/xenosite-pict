"""Co-displayed molecules share one coord scale."""

from __future__ import annotations

import math

import pytest

from xpict import Pict
from xpict.contracts.layout import AtomLayout, BondLayout, MoleculeLayout
from xpict.contracts.nodes import PictSpec
from xpict.draw.metrics import BOND_PX, coord_scale, shared_coord_scale
from xpict.draw.scene_builder import build_scene, normalize_coords


def _line_mol(bond_len: float, *, mol_id: str = "m") -> MoleculeLayout:
    """Two-atom molecule with a single bond of ``bond_len`` layout units."""
    return MoleculeLayout(
        id=mol_id,
        backend="native",
        atoms=[
            AtomLayout(index=0, element="C", x=0.0, y=0.0),
            AtomLayout(index=1, element="C", x=bond_len, y=0.0),
        ],
        bonds=[BondLayout(index=0, begin=0, end=1, order=1.0)],
    )


def test_shared_scale_pools_bond_lengths():
    short = _line_mol(1.0, mol_id="s")
    long = _line_mol(2.0, mol_id="l")
    # Per-mol scales differ…
    assert coord_scale(short) == pytest.approx(BOND_PX / 1.0)
    assert coord_scale(long) == pytest.approx(BOND_PX / 2.0)
    # …but the shared scale is one value from the pooled mean (1.5).
    shared = shared_coord_scale([short, long])
    assert shared == pytest.approx(BOND_PX / 1.5)
    _, sw, _ = normalize_coords(short, scale=shared)
    _, lw, _ = normalize_coords(long, scale=shared)
    # Drawn bond spans: short is 1.0*shared, long is 2.0*shared (plus pad).
    assert (lw - sw) == pytest.approx(shared)  # extra bond length in the longer mol


def test_build_scene_uses_shared_scale_for_codisplayed():
    short = _line_mol(1.0, mol_id="s")
    long = _line_mol(2.0, mol_id="l")
    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "s", "smiles": "CC"},
                {"id": "l", "smiles": "CC"},
            ],
            "halo": False,
        }
    )
    # Replace layouts with controlled bond lengths.
    scene = build_scene([short, long], doc.molecules, doc)
    assert len(scene.viewports) == 2
    # Content width minus 2*PAD: bond length in SVG px.
    from xpict.draw.drawable import _PAD

    span_s = scene.viewports[0].width - 2 * _PAD
    span_l = scene.viewports[1].width - 2 * _PAD
    assert span_l / span_s == pytest.approx(2.0)
    # Mean drawn bond is BOND_PX.
    assert (span_s + span_l) / 2 == pytest.approx(BOND_PX)


def test_reaction_pathway_mols_match_bond_scale():
    """Ethanol + aspirin in one diagram share drawn bond length."""
    from xpict.diagram.elk import layout_diagram_ex
    from xpict.draw.metrics import shared_coord_scale

    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "etoh", "smiles": "CCO"},
                {"id": "asa", "smiles": "CC(=O)Oc1ccccc1C(=O)O"},
            ],
            "diagram": {
                "kind": "reaction",
                "edges": [{"source": "etoh", "target": "asa", "arrow": "forward"}],
            },
        }
    )
    pict = Pict(backend="native")
    layouts = pict.layout(doc).molecules
    scale = shared_coord_scale(layouts)
    # Each mol's own scale may differ slightly; shared is one number.
    own = [coord_scale(lay) for lay in layouts]
    assert max(own) / min(own) >= 1.0
    place = layout_diagram_ex(layouts, doc)
    scene = build_scene(
        layouts,
        doc.molecules,
        doc,
        positions=place.positions,
        edge_paths=place.edge_paths,
        scale=scale,
    )
    assert len(scene.viewports) == 2
    # Spot-check: mean bond in each viewport's layout coords × shared scale ≈ BOND_PX.
    for lay in layouts:
        by = {a.index: a for a in lay.atoms}
        lens = [
            math.hypot(by[b.begin].x - by[b.end].x, by[b.begin].y - by[b.end].y)
            for b in lay.bonds
            if b.begin in by and b.end in by
        ]
        mean = sum(lens) / len(lens)
        assert mean * scale == pytest.approx(BOND_PX, rel=0.15)

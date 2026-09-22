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


def test_codisplayed_unlabeled_bonds_match_length():
    """Different viewport sizes, same unlabeled bond length in SVG space."""
    import re

    from xpict.contracts.scene import PathPrim
    from xpict.diagram.elk import layout_diagram_ex

    doc = PictSpec.model_validate(
        {
            "molecules": [
                {"id": "etoh", "smiles": "CCO"},
                {"id": "phenol", "smiles": "c1ccccc1O"},
            ],
            "diagram": {"kind": "grid", "columns": 2},
        }
    )
    pict = Pict(backend="native")
    layouts = pict.layout(doc).molecules
    scale = shared_coord_scale(layouts)
    place = layout_diagram_ex(layouts, doc)
    scene = build_scene(
        layouts, doc.molecules, doc, positions=place.positions, scale=scale
    )

    def unlabeled_skeleton_lens(vp):
        out = []
        for layer in vp.layers:
            for p in layer.primitives:
                if not isinstance(p, PathPrim) or not p.cls:
                    continue
                if "bond-skeleton" not in p.cls:
                    continue
                nums = [float(x) for x in re.findall(r"[-+]?\d*\.?\d+", p.d)]
                if len(nums) < 4:
                    continue
                x1, y1, x2, y2 = nums[:4]
                out.append(math.hypot(x2 - x1, y2 - y1))
        return out

    # Viewports differ in size (phenol is larger)…
    assert scene.viewports[0].width != pytest.approx(scene.viewports[1].width, rel=0.01)
    # …but the long unlabeled skeletons sit on BOND_PX.
    for vp in scene.viewports:
        lens = unlabeled_skeleton_lens(vp)
        assert max(lens) == pytest.approx(BOND_PX, abs=0.05)

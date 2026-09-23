"""Mol ``weight`` thickens backbone stroke and label glyph ink."""

from __future__ import annotations

import re

import pytest
from helpers import layout_backend
from pydantic import ValidationError

from xpict import DepictSpec, render
from xpict.draw.metrics import STROKE_PX, WEIGHT_AT_ONE, WEIGHT_MIN

_ROOT = re.compile(r"<svg\b([^>]*)>", re.DOTALL)
_ATTR = re.compile(r'([\w:-]+)="([^"]*)"')


def _bond_strokes(svg: str) -> list[float]:
    return [float(x) for x in re.findall(r'stroke-width="([0-9.]+)"', svg) if float(x) > 0]


def test_live_mol_accepts_weight():
    doc = DepictSpec.model_validate({"type": "mol", "smiles": "CCO", "weight": 1.5})
    assert doc.root.weight == pytest.approx(1.5)


def test_weight_defaults_to_one():
    doc = DepictSpec.model_validate({"type": "mol", "smiles": "CCO"})
    assert doc.root.weight == pytest.approx(1.0)


def test_weight_accepts_min():
    doc = DepictSpec.model_validate({"type": "mol", "smiles": "CCO", "weight": WEIGHT_MIN})
    assert doc.root.weight == pytest.approx(WEIGHT_MIN)


def test_weight_rejects_below_min():
    with pytest.raises(ValidationError):
        DepictSpec.model_validate({"type": "mol", "smiles": "CCO", "weight": 0.5})


def test_diagram_weight_maps_user_to_ink():
    from xpict.draw.metrics import diagram_weight

    assert diagram_weight() == pytest.approx(WEIGHT_AT_ONE)
    assert diagram_weight(None) == pytest.approx(WEIGHT_AT_ONE)
    assert diagram_weight(1.0) == pytest.approx(WEIGHT_AT_ONE)
    assert diagram_weight(WEIGHT_MIN) == pytest.approx(1.0)
    with pytest.raises(ValueError, match=">="):
        diagram_weight(0.5)
    with pytest.raises(ValueError, match=">="):
        diagram_weight(float("nan"))


def test_label_weight_standoff_grows_with_weight():
    from xpict.draw.metrics import label_weight_grow_px, label_weight_standoff_px

    assert label_weight_standoff_px(WEIGHT_MIN) == pytest.approx(0.0)
    stand1 = label_weight_standoff_px(1.0)
    assert stand1 > 0
    assert stand1 == pytest.approx(
        label_weight_grow_px(1.0) + 0.5 * (WEIGHT_AT_ONE * STROKE_PX - STROKE_PX)
    )


def test_halo_scales_with_sqrt_ink():
    from xpict.draw.metrics import (
        HALO_GAP_PX,
        HALO_STROKE,
        halo_gap_for_weight,
        halo_stroke_for_weight,
        halo_stroke_from_stroke,
        stroke_px_for_weight,
    )

    assert halo_gap_for_weight(WEIGHT_MIN) == pytest.approx(HALO_GAP_PX)
    assert halo_stroke_for_weight(WEIGHT_MIN) == pytest.approx(HALO_STROKE)
    assert halo_stroke_for_weight(1.0) == pytest.approx(HALO_STROKE * (WEIGHT_AT_ONE**0.5))
    # Absolute ink 4 → user 4/WEIGHT_AT_ONE → halo ×2.
    w_abs4 = 4.0 / WEIGHT_AT_ONE
    assert halo_gap_for_weight(w_abs4) == pytest.approx(2.0 * HALO_GAP_PX)
    assert halo_stroke_for_weight(w_abs4) == pytest.approx(2.0 * HALO_STROKE)
    assert halo_stroke_for_weight(2.0) < halo_stroke_from_stroke(stroke_px_for_weight(2.0))


def test_weight_one_matches_omitted():
    be = layout_backend()
    a = render({"type": "mol", "smiles": "CCO"}, backend=be)
    b = render({"type": "mol", "smiles": "CCO", "weight": 1.0}, backend=be)
    assert _bond_strokes(a)[:2] == pytest.approx(_bond_strokes(b)[:2], rel=1e-6)


def test_house_weight_is_one_point_five_times_regular_stem():
    be = layout_backend()
    house = render({"type": "mol", "smiles": "CCO"}, backend=be)
    light = render({"type": "mol", "smiles": "CCO", "weight": WEIGHT_MIN}, backend=be)
    heavy = render({"type": "mol", "smiles": "CCO", "weight": 2.0}, backend=be)
    house_sw = _bond_strokes(house)
    light_sw = _bond_strokes(light)
    heavy_sw = _bond_strokes(heavy)
    assert light_sw[0] == pytest.approx(STROKE_PX, rel=1e-3)
    assert house_sw[0] == pytest.approx(WEIGHT_AT_ONE * STROKE_PX, rel=1e-3)
    assert heavy_sw[0] == pytest.approx(2.0 * house_sw[0], rel=1e-3)


def _shade_radii(svg: str) -> list[float]:
    out: list[float] = []
    for m in re.finditer(r"<circle([^>]*)>", svg):
        attrs = m.group(1)
        if "shade" not in attrs:
            continue
        r = re.search(r'\br="([^"]+)"', attrs)
        if r:
            out.append(float(r.group(1)))
    return out


def test_weight_does_not_change_shade_radii():
    be = layout_backend()
    doc = {"type": "mol", "smiles": "CCO", "shade": {"atoms": [0.1, 0.5, 0.9]}}
    a = render(doc, backend=be)
    b = render({**doc, "weight": 2.0}, backend=be)
    assert _shade_radii(a)
    assert _shade_radii(a) == pytest.approx(_shade_radii(b), rel=1e-9)

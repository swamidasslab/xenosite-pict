"""Mol ``scale`` uniformly sizes the SVG diagram (font, stroke, geometry)."""

from __future__ import annotations

import re

import pytest
from helpers import layout_backend

from xpict import DepictSpec, render

_ROOT = re.compile(r"<svg\b([^>]*)>", re.DOTALL)
_ATTR = re.compile(r'([\w:-]+)="([^"]*)"')


def _root_wh(svg: str) -> tuple[float, float]:
    m = _ROOT.search(svg)
    assert m is not None
    attrs = dict(_ATTR.findall(m.group(1)))
    return float(attrs["width"].removesuffix("px")), float(attrs["height"].removesuffix("px"))


def test_live_mol_accepts_scale():
    doc = DepictSpec.model_validate({"type": "mol", "smiles": "CCO", "scale": 1.5})
    assert doc.root.scale == pytest.approx(1.5)


def test_scale_defaults_to_one():
    doc = DepictSpec.model_validate({"type": "mol", "smiles": "CCO"})
    assert doc.root.scale == pytest.approx(1.0)


def test_scale_one_matches_omitted():
    be = layout_backend()
    a = render({"type": "mol", "smiles": "CCO"}, backend=be)
    b = render({"type": "mol", "smiles": "CCO", "scale": 1.0}, backend=be)
    assert _root_wh(a) == pytest.approx(_root_wh(b), rel=1e-6)


def test_scale_two_doubles_intrinsic_size():
    be = layout_backend()
    base = render({"type": "mol", "smiles": "CCO"}, backend=be)
    scaled = render({"type": "mol", "smiles": "CCO", "scale": 2.0}, backend=be)
    bw, bh = _root_wh(base)
    sw, sh = _root_wh(scaled)
    assert sw == pytest.approx(2.0 * bw, rel=1e-4)
    assert sh == pytest.approx(2.0 * bh, rel=1e-4)

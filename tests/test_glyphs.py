"""Tests for glyph / text-box intrinsic size helpers."""

from __future__ import annotations

import pytest

from xenosite.pict.draw.font_face import bundled_font_path
from xenosite.pict.draw.glyphs import geom_to_svg_d, label_halo_path_d, label_outline
from xenosite.pict.draw.metrics import (
    FONT_PX,
    FONT_STEM_EM,
    LABEL_GAP_PX,
    STROKE_PX,
    label_clearance,
)
from xenosite.pict.draw.text_metrics import (
    face_metrics,
    glyph_metrics,
    label_baseline_offset,
    measure_text,
    stem_width,
    text_box,
)


def test_bundled_liberation_sans_is_present():
    path = bundled_font_path()
    assert path.is_file()
    assert path.suffix.lower() == ".ttf"
    assert path.stat().st_size > 100_000


def test_face_metrics_match_liberation_tables():
    face = face_metrics()
    assert face.upem == 2048
    assert face.ascent == pytest.approx(1854)
    assert face.descent == pytest.approx(-434)
    assert face.cap_height > 0.5 * face.upem
    assert face.stem_em == pytest.approx(FONT_STEM_EM, rel=0.05)


def test_glyph_metrics_for_common_labels():
    h = glyph_metrics("H")
    assert h.advance == pytest.approx(1479)
    assert h.has_ink
    assert h.ink_height > 1000
    o = glyph_metrics("O")
    assert o.ink_width > o.advance * 0.5
    space = glyph_metrics(" ")
    assert space.advance > 0


def test_measure_text_oh_and_scientific_symbols():
    for text in ("OH", "NH2", "Cl", "Br", "Na+", "°", "µ"):
        m = measure_text(text, FONT_PX)
        assert m.advance > 0
        assert m.typo.height > 0
        assert m.ink is not None and m.ink.width > 0


def test_text_box_respects_anchor_and_pad():
    m = measure_text("OH", FONT_PX)
    ink_mid, _, _ = m.at(100.0, 50.0, anchor="middle")
    box = text_box("OH", 100.0, 50.0, anchor="middle", pad=2.0)
    assert ink_mid is not None
    assert box.xmin == pytest.approx(ink_mid.xmin - 2.0)
    assert box.xmax == pytest.approx(ink_mid.xmax + 2.0)
    assert abs(box.cx - 100.0) < m.advance * 0.15


def test_label_clearance_uses_real_advance():
    m = measure_text("OH", FONT_PX)
    assert label_clearance("OH") == pytest.approx(m.clearance())
    assert label_clearance("COOH") > label_clearance("O")
    # Gap past the ink is the shared label/bond air, not the tiny stem width.
    assert m.clearance() - 0.5 * m.advance == pytest.approx(LABEL_GAP_PX)


def test_label_gap_matches_rdkit_order():
    """Bond/label air is ~1 px at BOND_PX=20 (RDKit measured ~0.9–1.0)."""
    assert LABEL_GAP_PX == pytest.approx(1.0, abs=0.05)
    assert LABEL_GAP_PX < 0.1 * FONT_PX * 2  # well under the old 5 px moat


def test_stem_width_matches_default_stroke():
    assert stem_width(FONT_PX) == pytest.approx(STROKE_PX, rel=0.05)


def test_label_baseline_centers_caps_on_atom():
    off = label_baseline_offset(FONT_PX)
    m = measure_text("O", FONT_PX)
    ink, _, _ = m.at(0.0, off, anchor="middle")
    assert ink is not None
    assert ink.cy == pytest.approx(0.0, abs=0.75)


def test_halo_buffer_matches_label_gap():
    ink = label_outline("O", 10.0, 20.0)
    assert ink is not None
    d = label_halo_path_d("O", 10.0, 20.0)
    assert d and "M" in d and "Z" in d
    grown = ink.buffer(LABEL_GAP_PX)
    assert grown.area > ink.area
    assert geom_to_svg_d(grown).count("L") > 8
    # Default halo path uses LABEL_GAP_PX (same air as bond pullback).
    assert label_halo_path_d("O", 10.0, 20.0, buffer_px=LABEL_GAP_PX) == d

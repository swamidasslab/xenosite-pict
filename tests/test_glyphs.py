"""Tests for bundled-font glyph outlines and buffered label halos."""

from __future__ import annotations

from xenosite.pict.draw.glyphs import (
    bundled_font_path,
    geom_to_svg_d,
    label_halo_path_d,
    label_outline,
)
from xenosite.pict.draw.metrics import FONT_PX, HALO_STROKE


def test_bundled_liberation_sans_is_present():
    path = bundled_font_path()
    assert path.is_file()
    assert path.suffix.lower() == ".ttf"
    assert path.stat().st_size > 100_000


def test_label_outline_covers_scientific_symbols():
    for text in ("OH", "NH2", "Cl", "Br", "Na+", "°", "µ"):
        geom = label_outline(text, 0.0, 0.0, font_size=FONT_PX)
        assert geom is not None and not geom.is_empty, text


def test_halo_buffer_grows_past_outline():
    ink = label_outline("O", 10.0, 20.0)
    assert ink is not None
    d = label_halo_path_d("O", 10.0, 20.0, buffer_px=HALO_STROKE * 0.5)
    assert d and "M" in d and "Z" in d
    grown = ink.buffer(HALO_STROKE * 0.5)
    assert grown.area > ink.area
    assert geom_to_svg_d(grown).count("L") > 8

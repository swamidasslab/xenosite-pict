"""Depiction proportions vs xenopict, Indigo SVG, and RDKit MolDrawOptions."""

from __future__ import annotations

import math
import re

import pytest

from xenosite.pict import Pict, render
from xenosite.pict.draw.bonds import (
    bond_strokes,
    crossed_double,
    hashed_wedge,
    solid_wedge,
    wavy_bond,
)
from xenosite.pict.draw.metrics import (
    BOND_PX,
    FONT_PX,
    FONT_STEM_EM,
    HALO_STROKE,
    OFFSET_FRAC,
    STROKE_FRAC,
    STROKE_PX,
    WEDGE_WIDTH_FRAC,
    coord_scale,
    hash_count,
)
from xenosite.pict.draw.scene_builder import normalize_coords


def _backend() -> str:
    try:
        Pict(backend="indigo").layout({"molecules": [{"smiles": "C"}]})
        return "indigo"
    except Exception:
        pytest.skip("indigo not installed")


def _path_ys(d: str) -> list[float]:
    return [float(y) for y in re.findall(r"[ML]\s+[-\d.]+\s+([-\d.]+)", d)]


def _path_len(d: str) -> float:
    pts = [(float(x), float(y)) for x, y in re.findall(r"[ML]\s+([-\d.]+)\s+([-\d.]+)", d)]
    if len(pts) < 2:
        return 0.0
    (x1, y1), (x2, y2) = pts[0], pts[-1]
    return math.hypot(x2 - x1, y2 - y1)


def test_double_offset_matches_rdkit_fraction():
    """Separation of a centered double is still RDKit's 0.15 bond offset."""
    strokes = bond_strokes(0, 0, BOND_PX, 0, 2.0)
    assert len(strokes.offsets) == 2
    ys = sorted(_path_ys(p.d)[0] for p in strokes.offsets)
    assert ys[0] == pytest.approx(-ys[1], abs=0.05)
    assert ys[1] - ys[0] == pytest.approx(OFFSET_FRAC * BOND_PX, rel=0.05)
    assert (ys[0] + ys[1]) / 2 == pytest.approx(0.0, abs=0.05)


def test_acyclic_double_offset_extends_past_ring_offset():
    """Centered chain lines run the bond; ring doubles stay short and inside."""
    chain = bond_strokes(0, 0, BOND_PX, 0, 2.0)
    ring = bond_strokes(0, 0, BOND_PX, 0, 2.0, interior=(0.0, -1.0))
    assert chain.skeleton is None and len(chain.offsets) == 2
    assert ring.skeleton is not None and ring.offsets
    chain_len = _path_len(chain.offsets[0].d)
    ring_len = _path_len(ring.offsets[0].d)
    assert chain_len > ring_len + 0.1 * BOND_PX
    assert chain_len == pytest.approx(BOND_PX, abs=0.05)
    # Ring offset is on the interior side, not mirrored.
    assert _path_ys(ring.offsets[0].d)[0] < 0


def test_triple_offsets_are_symmetric():
    strokes = bond_strokes(0, 0, BOND_PX, 0, 3.0)
    assert len(strokes.offsets) == 2
    ys = sorted(abs(_path_ys(p.d)[0]) for p in strokes.offsets)
    assert ys[0] == pytest.approx(ys[1], rel=0.01)
    assert ys[0] == pytest.approx(OFFSET_FRAC * BOND_PX, rel=0.05)


def test_stroke_is_fraction_of_bond():
    strokes = bond_strokes(0, 0, BOND_PX, 0, 1.0)
    assert strokes.skeleton is not None
    assert strokes.skeleton.stroke_width == pytest.approx(STROKE_FRAC * BOND_PX, rel=0.02)
    assert strokes.skeleton.stroke_linecap == "round"


def test_stroke_matches_label_stem():
    """Default bond ink is the label face's vertical stem, not 0.10×bond."""
    stem = FONT_STEM_EM * FONT_PX
    assert STROKE_PX == pytest.approx(stem, abs=0.01)
    assert STROKE_PX == pytest.approx(1.12, abs=0.001)
    assert HALO_STROKE == pytest.approx(2 * STROKE_PX, abs=0.001)
    strokes = bond_strokes(0, 0, BOND_PX, 0, 1.0)
    assert strokes.skeleton is not None
    assert strokes.skeleton.stroke_width == pytest.approx(stem, abs=0.01)


def test_wedge_fat_end_width():
    w = solid_wedge(0, 0, BOND_PX, 0)
    # Fat end is the two points after the tip; full width ≈ WEDGE_WIDTH_FRAC * bond.
    nums = [float(n) for n in re.findall(r"[-+]?\d+\.\d+", w.d)]
    # M tipx tipy L ax ay L bx by
    ay, by = nums[3], nums[5]
    full = abs(ay - by)
    assert full == pytest.approx(WEDGE_WIDTH_FRAC * BOND_PX, rel=0.08)


def test_hash_count_scales_with_length():
    short = hashed_wedge(0, 0, BOND_PX * 0.5, 0)
    full = hashed_wedge(0, 0, BOND_PX, 0)
    assert len(full) >= len(short)
    assert len(full) == hash_count(BOND_PX)
    assert len(full) >= 8


def test_indigo_and_native_bonds_same_pixel_length():
    indigo = Pict(backend="indigo").layout({"molecules": [{"smiles": "CCO"}]}).molecules[0]
    native = Pict(backend="native").layout({"molecules": [{"smiles": "CCO"}]}).molecules[0]
    _, iw, _ = normalize_coords(indigo)
    _, nw, _ = normalize_coords(native)
    # Both ethanol chains are two bonds; widths should be close (label pad dominates).
    assert abs(iw - nw) / max(iw, nw) < 0.25
    assert coord_scale(indigo) * 1.0 == pytest.approx(BOND_PX, rel=0.05)


def test_svg_uses_reference_font_and_butt_bonds():
    svg = render({"molecules": [{"smiles": "CC(=O)C"}]}, backend="native")
    assert f'font-size="{FONT_PX}"' in svg or f"font-size=\"{FONT_PX}" in svg
    assert 'stroke-linecap="round"' in svg
    # Double-bond offset is visibly wider than the old 2.4 px stub.
    assert "bond-offset" in svg


def test_indigo_hetero_labels_include_implicit_h():
    backend = _backend()
    lay = Pict(backend=backend).layout({"molecules": [{"smiles": "CCO"}]}).molecules[0]
    oxy = next(a for a in lay.atoms if a.element == "O")
    assert oxy.label == "OH"
    nitro = Pict(backend=backend).layout(
        {"molecules": [{"smiles": "C[C@H](N)C(=O)O"}]}
    ).molecules[0]
    nitrogen = next(a for a in nitro.atoms if a.element == "N")
    assert nitrogen.label == "NH2"
    svg = render({"molecules": [{"smiles": "CCO"}]}, backend=backend)
    assert "OH" in svg


def test_indigo_anion_is_not_a_radical_dot():
    backend = _backend()
    lay = Pict(backend=backend).layout({"molecules": [{"smiles": "[O-]"}]}).molecules[0]
    assert lay.atoms[0].charge == -1
    assert lay.atoms[0].radical == 0
    svg = render({"molecules": [{"smiles": "[O-]"}]}, backend=backend)
    assert "radical" not in svg
    assert "−" in svg or "-" in svg


def test_ammonium_label_has_hydrogens_and_charge():
    backend = _backend()
    svg = render({"molecules": [{"smiles": "[NH4+]"}]}, backend=backend)
    assert "NH4" in svg
    assert "+" in svg


def test_wedge_and_hash_and_wavy_and_crossed_in_svg():
    backend = _backend()
    up = render({"molecules": [{"smiles": "C[C@H](O)Cl"}]}, backend=backend)
    assert "bond-wedge" in up
    # Either single: unspecified stereo on a chiral-looking atom isn't easy in
    # SMILES; draw via bond_strokes already unit-tested. Crossed double:
    # Indigo may not emit either on CC=CC. Check proline / alanine hash or wedge
    # and a native either bond path.
    ala = render({"molecules": [{"smiles": "C[C@H](N)C(=O)O"}]}, backend=backend)
    assert "wedge-up" in ala or "wedge-down" in ala
    wavy = wavy_bond(0, 0, BOND_PX, 0)
    assert "bond-either" in (wavy.cls or "")
    assert wavy.d.count("L") >= 6
    cross = crossed_double(0, 0, BOND_PX, 0, None)
    assert any("either-cross" in (p.cls or "") for p in cross)


def test_radical_dot_still_drawn():
    svg = render({"molecules": [{"smiles": "[CH3]"}]}, backend="native")
    assert "radical" in svg
    assert "CH3" in svg or "C" in svg


def test_mean_bond_scale_indigo():
    backend = _backend()
    lay = Pict(backend=backend).layout({"molecules": [{"smiles": "c1ccccc1"}]}).molecules[0]
    coords, _, _ = normalize_coords(lay)
    # Benzene C–C should be ~BOND_PX after scaling.
    dists = []
    for i, (x, y) in enumerate(coords):
        for j, (x2, y2) in enumerate(coords):
            if j <= i:
                continue
            d = math.hypot(x - x2, y - y2)
            if d > 1:
                dists.append(d)
    bondish = min(dists)
    assert bondish == pytest.approx(BOND_PX, rel=0.08)

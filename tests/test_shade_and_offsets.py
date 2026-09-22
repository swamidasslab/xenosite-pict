"""Shade colormap + multi-bond offset vs xenopict."""

from __future__ import annotations

import math
import re

import pytest

from xpict import Pict, render
from xpict.draw.bonds import bond_strokes, multi_bond_offset, shorten
from xpict.draw.colormap import colormap_rgb, sample_colormap
from xpict.draw.drawable import LAYER_ORDER, _shade_rgb
from xpict.draw.metrics import BOND_PX, OFFSET_PX, label_clearance


def test_layer_order_matches_xenopict_shading_under_halo():
    """xenopict groups: shading → mol_halo → lines; shade must not cover halo."""
    assert LAYER_ORDER[0] == "shading"
    assert LAYER_ORDER[1] == "halo"
    assert LAYER_ORDER.index("shading") < LAYER_ORDER.index("halo")
    assert LAYER_ORDER.index("halo") < LAYER_ORDER.index("bonds")


def test_xenosite_colormap_matches_xenopict_stops():
    """Sequential xenosite: white → blue → green → yellow → orange-red."""
    assert _shade_rgb(0.0) == "rgb(255,255,255)"
    assert colormap_rgb(0.0) == "rgb(255,255,255)"
    r, g, b = sample_colormap(0.25)
    assert b > r and b > g  # blue lobe
    r, g, b = sample_colormap(0.5)
    assert g > r and g > b  # green mid
    r, g, b = sample_colormap(1.0)
    assert r > 0.9 and g < 0.4 and b < 0.1  # hot end
    # Non-diverging negatives clip to white (matplotlib / xenopict default).
    assert colormap_rgb(-0.5) == "rgb(255,255,255)"
    # Diverging remaps [-1,1] → [0,1].
    assert colormap_rgb(-1.0, diverging=True) == "rgb(255,255,255)"
    assert colormap_rgb(1.0, diverging=True) == colormap_rgb(1.0)


def test_svg_shade_uses_xenosite_not_homemade_pink():
    svg = render(
        {
            "molecules": [
                {
                    "smiles": "c1ccccc1",
                    "shade": {"atoms": [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]},
                }
            ]
        },
        backend="native",
    )
    fills = re.findall(r'class="shade"[^>]*fill="(rgb\([^"]+\))"|fill="(rgb\([^"]+\))"[^>]*class="shade"', svg)
    flat = [a or b for a, b in fills]
    assert flat
    # Homemade pink was rgb(255,k,k). xenosite mid tones are not pink.
    assert any(
        not (f.startswith("rgb(255,") and f.count(",") == 2 and "255,255" not in f)
        for f in flat
    )
    assert any("rgb(0," in f or ",0," in f or f.startswith("rgb(0") for f in flat) or any(
        int(re.findall(r"\d+", f)[2]) > int(re.findall(r"\d+", f)[0]) for f in flat
    )


def test_svg_layer_order_shading_before_halo():
    svg = render(
        {"molecules": [{"smiles": "CCO", "shade": {"atoms": [0.2, 0.5, 0.9]}}]},
        backend="native",
    )
    i_shade = svg.index('class="layer-shading"')
    i_halo = svg.index('class="xpict-halo"')
    i_bonds = svg.index('class="layer-bonds"')
    assert i_shade < i_halo < i_bonds


def test_multi_bond_offset_ignores_label_shortening():
    """Carbonyl insets must not collapse double spacing below OFFSET_PX."""
    g = label_clearance("O")
    x1, y1, x2, y2 = shorten(0.0, 0.0, BOND_PX, 0.0, 0.0, g)
    length = math.hypot(x2 - x1, y2 - y1)
    assert length < BOND_PX
    # Visible stroke ~10 px: old code used min(OFFSET_PX, length*0.22) ≈ 2.2.
    stub_len = 10.0
    assert stub_len * 0.22 < OFFSET_PX
    assert stub_len >= 2.0 * OFFSET_PX
    assert multi_bond_offset(stub_len) == pytest.approx(OFFSET_PX)
    strokes = bond_strokes(0.0, 0.0, stub_len, 0.0, 2.0)
    ys = sorted(
        float(re.findall(r"[ML]\s+[-\d.]+\s+([-\d.]+)", p.d)[0]) for p in strokes.offsets
    )
    assert ys[1] - ys[0] == pytest.approx(OFFSET_PX, abs=0.05)


def test_ring_double_offset_distance_is_full_offset_px():
    strokes = bond_strokes(0, 0, BOND_PX, 0, 2.0, interior=(0.0, -1.0))
    assert strokes.skeleton is not None and strokes.offsets
    y = float(re.findall(r"[ML]\s+[-\d.]+\s+([-\d.]+)", strokes.offsets[0].d)[0])
    assert y == pytest.approx(-OFFSET_PX, abs=0.05)


def _backend() -> str:
    """MVP layout backend (indigo is out of scope for now)."""
    return "native"


def test_rendered_ring_doubles_sit_inside():
    backend = _backend()
    from xpict.draw.drawable import normalize_coords
    from xpict.draw.rings import bond_interior_normals, find_sssr

    for smiles in ("c1ccccc1", "O=C1C=CC(=O)C=C1", "c1ccc2ccccc2c1"):
        lay = Pict(backend=backend).layout({"molecules": [{"smiles": smiles}]}).molecules[0]
        coords, _, _ = normalize_coords(lay)
        rings = find_sssr(lay)
        cbi = {a.index: coords[i] for i, a in enumerate(lay.atoms)}
        normals = bond_interior_normals(rings, cbi)
        atom_pos = {a.index: i for i, a in enumerate(lay.atoms)}
        for bond in lay.bonds:
            if bond.order < 1.5:
                continue
            key = (min(bond.begin, bond.end), max(bond.begin, bond.end))
            n = normals.get(key)
            if n is None:
                continue
            owners = [r for r in rings if key in r.bonds]
            if not owners:
                continue
            ring = min(owners, key=lambda r: r.size)
            cx = sum(cbi[i][0] for i in ring.atoms) / len(ring.atoms)
            cy = sum(cbi[i][1] for i in ring.atoms) / len(ring.atoms)
            x1, y1 = coords[atom_pos[bond.begin]]
            x2, y2 = coords[atom_pos[bond.end]]
            mx, my = (x1 + x2) / 2, (y1 + y2) / 2
            d0 = math.hypot(mx - cx, my - cy)
            d1 = math.hypot(mx + n[0] * OFFSET_PX - cx, my + n[1] * OFFSET_PX - cy)
            assert d1 < d0 - 0.5, f"{smiles} bond {key} offset outside ring"

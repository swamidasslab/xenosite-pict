"""Hard-case regression tests grown from depiction failures.

Each case encodes a concrete bug we hit while derisking:
- Indigo aromatic order=4 drawn as triple bonds
- Double-bond offsets on the outside of rings
- Shade min/max remapping that turned zeros into full blue
- Plot-dot disks for near-zero scores
- Missing clear SVG background (host page shows through)
- Halo duplicated per multi-bond stroke (hollow-tube look)

Also encodes ring-layout hard cases from CDK/RDKit practice: bridged and
cage systems where not every SSSR face can be a regular polygon.

Add new SMILES / assertions here when a depiction failure is found —
do not delete cases once they pass.
"""

from __future__ import annotations

import math
import re
from collections import Counter

import pytest

from xpict import Pict, render
from xpict.draw.metrics import HALO_STROKE
from xpict.draw.plotdot import PlotDot
from xpict.draw.rings import (
    HARD_RING_CASES,
    RingAttachment,
    all_rings_can_be_regular_polygons,
    find_sssr,
    ring_pair_relations,
    ring_system_is_overconstrained,
)
from xpict.draw.bonds import bond_paths
from xpict.draw.drawable import (
    _normalize_shade_scores,
    _shade_rgb,
    normalize_coords,
)
from xpict.draw.rings import bond_interior_normals, find_sssr


HARD_CASES: list[dict] = [
    {"id": "benzene", "smiles": "c1ccccc1", "notes": "aromatic hexagon; Kekulize; doubles inside"},
    {"id": "phenol", "smiles": "c1ccccc1O", "notes": "heteroatom label + aromatic"},
    {"id": "aspirin", "smiles": "CC(=O)Oc1ccccc1C(=O)O", "notes": "ortho + carbonyls + aromatic"},
    {"id": "indole", "smiles": "c1ccc2[nH]ccc2c1", "notes": "fused 6+5 aromatic"},
    {"id": "naphthalene", "smiles": "c1ccc2ccccc2c1", "notes": "fused aromatics share bond"},
    {"id": "biphenyl", "smiles": "c1ccc(-c2ccccc2)cc1", "notes": "two rings, single-bond linker"},
    {"id": "acetone", "smiles": "CC(=O)C", "notes": "acyclic carbonyl"},
    {"id": "pyridine", "smiles": "c1ccncc1", "notes": "aromatic heterocycle"},
    {"id": "nitrobenzene", "smiles": "c1ccc(cc1)[N+](=O)[O-]", "notes": "charged + aromatic"},
    {"id": "caffeine", "smiles": "Cn1cnc2c1c(=O)n(c(=O)n2C)C", "notes": "fused heteroaromatic + carbonyls"},
]


def _chem_backend() -> str:
    try:
        Pict(backend="indigo").layout({"molecules": [{"smiles": "CCO"}]})
        return "indigo"
    except Exception:
        return "native"


def _bond_stroke_counts(svg: str) -> Counter[str]:
    return Counter(re.findall(r'class="(bond-\d+)\b', svg))


def _path_midpoint(d: str) -> tuple[float, float]:
    nums = [float(x) for x in re.findall(r"[-+]?\d*\.?\d+(?:[eE][-+]?\d+)?", d)]
    assert len(nums) >= 4, d
    return ((nums[0] + nums[2]) / 2, (nums[1] + nums[3]) / 2)


def test_shade_normalize_nonnegative_keeps_zero_white():
    zs = [0.0, 0.0, 0.9, 0.0, 0.5]
    out = _normalize_shade_scores(zs, vmin=0.0, vmax=1.0)
    assert out[0] == pytest.approx(0.0)
    assert out[2] == pytest.approx(0.9)
    assert out[4] == pytest.approx(0.5)
    assert _shade_rgb(out[0]) == "rgb(255,255,255)"


def test_shade_normalize_diverging_preserves_sign():
    zs = [-0.8, 0.0, 0.4]
    out = _normalize_shade_scores(zs, vmin=-0.8, vmax=0.4)
    assert out[0] == pytest.approx(-1.0)
    assert out[1] == pytest.approx(0.0)
    assert out[2] == pytest.approx(0.5)


def test_plotdot_skips_near_zero_scores():
    dots = PlotDot()([0.0, 0.01, 0.9], [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)])
    assert all(abs(color) >= 0.05 for _, color, _ in dots)
    assert any(abs(color) > 0.5 for _, color, _ in dots)


def test_aromatic_layout_is_kekulized():
    backend = _chem_backend()
    layout = (
        Pict(backend=backend)
        .layout({"molecules": [{"smiles": "c1ccccc1"}]})
        .molecules[0]
    )
    orders = [b.order for b in layout.bonds]
    assert all(o < 3.5 for o in orders), f"aromatic order leaked: {orders}"
    doubles = sum(1 for o in orders if o >= 1.5)
    singles = sum(1 for o in orders if o < 1.5)
    assert doubles == 3 and singles == 3, orders


def test_no_aromatic_drawn_as_triple_strokes():
    backend = _chem_backend()
    for case in HARD_CASES:
        svg = render({"molecules": [{"smiles": case["smiles"]}]}, backend=backend)
        bad = {k: v for k, v in _bond_stroke_counts(svg).items() if v >= 3}
        assert not bad, f"{case['id']}: triple-looking strokes {bad}"


def test_acetone_carbonyl_is_double_not_triple():
    backend = _chem_backend()
    svg = render({"molecules": [{"smiles": "CC(=O)C"}]}, backend=backend)
    counts = _bond_stroke_counts(svg)
    assert 2 in counts.values()
    assert all(v <= 2 for v in counts.values()), counts


def test_double_bond_offset_prefers_ring_interior():
    backend = _chem_backend()
    pict = Pict(backend=backend)
    layout = pict.layout({"molecules": [{"smiles": "c1ccccc1"}]}).molecules[0]
    coords, _, _ = normalize_coords(layout)
    rings = find_sssr(layout)
    coords_by_index = {a.index: coords[i] for i, a in enumerate(layout.atoms)}
    normals = bond_interior_normals(rings, coords_by_index)
    assert normals, "expected ring normals for benzene"

    double = next(b for b in layout.bonds if b.order >= 1.5)
    key = (
        (double.begin, double.end)
        if double.begin < double.end
        else (double.end, double.begin)
    )
    interior = normals[key]
    atom_pos = {a.index: i for i, a in enumerate(layout.atoms)}
    x1, y1 = coords[atom_pos[double.begin]]
    x2, y2 = coords[atom_pos[double.end]]
    paths = bond_paths(x1, y1, x2, y2, double.order, interior=interior)
    assert len(paths) == 2

    cx = sum(x for x, _ in coords) / len(coords)
    cy = sum(y for _, y in coords) / len(coords)
    mid_main = _path_midpoint(paths[0].d)
    mid_off = _path_midpoint(paths[1].d)
    d_main = math.hypot(mid_main[0] - cx, mid_main[1] - cy)
    d_off = math.hypot(mid_off[0] - cx, mid_off[1] - cy)
    assert d_off < d_main, (
        f"offset line outside ring: d_off={d_off:.2f} d_main={d_main:.2f}"
    )


def test_svg_background_is_clear_by_default():
    backend = _chem_backend()
    svg = render({"molecules": [{"smiles": "c1ccccc1O"}]}, backend=backend)
    assert "pict-background" not in svg
    assert "background-color:#ffffff" not in svg
    assert "background-color:#fff" not in svg
    # Root <svg> should not force an opaque canvas style.
    root = re.search(r"<svg\b[^>]*>", svg)
    assert root is not None
    assert "background-color" not in root.group(0)


def test_halo_follows_each_bond_stroke():
    """Every bond stroke gets a halo derived via halo_from_shapes.

    Not one halo per bond: Kekulé offsets get their own knockout.
    """
    backend = _chem_backend()
    svg = render({"molecules": [{"smiles": "c1ccccc1"}]}, backend=backend)
    n_bonds = len(
        Pict(backend=backend)
        .layout({"molecules": [{"smiles": "c1ccccc1"}]})
        .molecules[0]
        .bonds
    )
    n_halo = len(re.findall(r"<path[^>]*class=\"halo\"", svg))
    n_ink = len(re.findall(r"bond-skeleton|bond-offset|bond-wedge", svg))
    assert n_halo == n_ink
    assert n_halo > n_bonds  # offsets are haloed too


def test_halo_includes_buffered_label_glyphs():
    """Label knockout is halo_from_shapes(glyph ink); counters stay open."""
    backend = _chem_backend()
    svg = render({"molecules": [{"smiles": "CCO"}]}, backend=backend)
    assert not re.search(r'<circle[^>]*label-halo', svg)
    assert not re.search(r'<text[^>]*label-halo', svg)
    halo = re.search(r'<path[^>]*class="halo label-halo"[^/]*/>', svg)
    assert halo is not None
    assert 'fill="#fff"' in halo.group(0)
    assert 'stroke="none"' in halo.group(0)
    # Atom label ink is a glyph path (shared shapes engine).
    assert re.search(
        r'<path[^>]*data-text="OH"[^>]*class="[^"]*\blabel\b',
        svg,
    ) or re.search(
        r'<path[^>]*class="[^"]*\blabel\b[^"]*"[^>]*data-text="OH"',
        svg,
    )
    d = re.search(r'\bd="([^"]+)"', halo.group(0))
    assert d is not None and d.group(1).count("L") > 8


def test_shade_zeros_do_not_paint_full_disks():
    backend = _chem_backend()
    pict = Pict(backend=backend)
    layout = pict.layout(
        {"molecules": [{"smiles": "CC(=O)Oc1ccccc1C(=O)O"}]}
    ).molecules[0]
    n = len(layout.atoms)
    shade = [0.0] * n
    for i, a in enumerate(layout.atoms):
        if a.element == "O":
            shade[i] = 0.95
    svg = pict.render(
        {
            "molecules": [
                {
                    "smiles": "CC(=O)Oc1ccccc1C(=O)O",
                    "shade": {"atoms": shade, "vmin": 0.0, "vmax": 1.0},
                }
            ]
        }
    )
    n_shade = len(re.findall(r'class="shade"', svg))
    assert n_shade > 0
    assert n_shade < n * 3, f"too many shade disks ({n_shade}) for {n} atoms"


@pytest.mark.parametrize("case", HARD_CASES, ids=[c["id"] for c in HARD_CASES])
def test_hard_case_renders_without_triple_bonds(case: dict):
    backend = _chem_backend()
    svg = render({"molecules": [{"smiles": case["smiles"]}]}, backend=backend)
    assert "<svg" in svg
    assert "xpict-mol" in svg
    counts = _bond_stroke_counts(svg)
    assert counts, f"{case['id']}: no bonds drawn"
    assert max(counts.values()) <= 2, f"{case['id']}: {counts}"


def test_fused_systems_admit_regular_polygons():
    """Naphthalene/indole/spiro: CDK FUSED/SPIRO — sequential regular n-gons OK."""
    backend = _chem_backend()
    for smiles in ("c1ccc2ccccc2c1", "c1ccc2[nH]ccc2c1", "C1CCC2(CC1)CCCC2"):
        layout = (
            Pict(backend=backend)
            .layout({"molecules": [{"smiles": smiles}]})
            .molecules[0]
        )
        rings = find_sssr(layout)
        assert rings
        assert all_rings_can_be_regular_polygons(rings), smiles
        assert not ring_system_is_overconstrained(rings), smiles


def test_bridged_and_cage_systems_are_overconstrained():
    """Norbornane/cubane/adamantane: cannot force every SSSR face regular."""
    backend = _chem_backend()
    for case in HARD_RING_CASES:
        layout = (
            Pict(backend=backend)
            .layout({"molecules": [{"smiles": case["smiles"]}]})
            .molecules[0]
        )
        rings = find_sssr(layout)
        assert rings, case["id"]
        assert not all_rings_can_be_regular_polygons(rings), case["id"]
        assert ring_system_is_overconstrained(rings), case["id"]


@pytest.mark.parametrize(
    "case", HARD_RING_CASES, ids=[c["id"] for c in HARD_RING_CASES]
)
def test_hard_ring_case_still_renders(case: dict):
    backend = _chem_backend()
    svg = render({"molecules": [{"smiles": case["smiles"]}]}, backend=backend)
    assert "<svg" in svg
    counts = _bond_stroke_counts(svg)
    assert counts
    assert max(counts.values()) <= 2, f"{case['id']}: {counts}"


def test_norbornane_pair_is_cdk_bridged():
    backend = _chem_backend()
    layout = (
        Pict(backend=backend)
        .layout({"molecules": [{"smiles": "C1CC2CCC1C2"}]})
        .molecules[0]
    )
    rings = find_sssr(layout)
    assert len(rings) >= 2
    rels = ring_pair_relations(rings)
    assert any(r.kind == RingAttachment.BRIDGED for r in rels)

"""Stereo + skeleton-first bond drawing tests (RDKit/Indigo conventions)."""

from __future__ import annotations

import math
import re

import pytest

from xpict import Pict, render
from xpict.draw.bonds import (
    DrawnBond,
    bond_strokes,
    hashed_wedge,
    join_centered_multibonds,
    solid_wedge,
)
from xpict.draw.metrics import OFFSET_PX


def _backend() -> str:
    """MVP layout backend (indigo is out of scope for now)."""
    return "native"


def test_skeleton_then_offset_for_double():
    strokes = bond_strokes(0, 0, 20, 0, 2.0)
    # Acyclic double is centered: no axis line, one stroke each side.
    assert strokes.skeleton is None
    assert len(strokes.offsets) == 2
    assert all("bond-offset" in (p.cls or "") for p in strokes.offsets)
    ys = sorted(_ys(p.d) for p in strokes.offsets)
    assert ys[0] == pytest.approx(-ys[1], abs=0.05)
    assert abs(ys[0]) == pytest.approx(OFFSET_PX / 2, abs=0.05)
    for path in strokes.offsets:
        assert "0.00" in path.d and "20.00" in path.d


def _ys(d: str) -> float:
    nums = [float(n) for n in re.findall(r"[-+]?\d+\.\d+", d)]
    return nums[1]


def test_triple_has_skeleton_and_two_offsets():
    strokes = bond_strokes(0, 0, 30, 0, 3.0)
    assert strokes.skeleton is not None
    assert "bond-skeleton" in (strokes.skeleton.cls or "")
    assert len(strokes.offsets) == 2
    # Centered: the axis line is y=0 and the offsets are opposite.
    skel_y = _ys(strokes.skeleton.d)
    assert skel_y == pytest.approx(0.0, abs=0.05)
    off_y = sorted(_ys(p.d) for p in strokes.offsets)
    assert off_y[0] == pytest.approx(-off_y[1], abs=0.05)


def test_solid_wedge_tip_at_begin():
    w = solid_wedge(1.0, 2.0, 11.0, 2.0)
    assert w.d.startswith("M 1.00 2.00")
    assert w.fill == "#111"
    assert "wedge-up" in (w.cls or "")


def test_hashed_wedge_has_multiple_dashes():
    dashes = hashed_wedge(0, 0, 20, 0)
    assert len(dashes) >= 4


def test_down_hash_is_wide_at_stereocenter():
    """Away-bond: thick end on the stereocenter, thin end on the substituent."""
    dashes = hashed_wedge(0, 0, 40, 0)
    widths = []
    centers = []
    for dash in dashes:
        nums = [float(n) for n in re.findall(r"[-+]?\d+\.\d+", dash.d)]
        widths.append(abs(nums[1] - nums[3]))
        centers.append((nums[0] + nums[2]) / 2)
    assert centers[0] < centers[-1]
    assert widths[0] > widths[-1] * 2


def test_stereo_up_replaces_skeleton():
    strokes = bond_strokes(0, 0, 10, 0, 1.0, stereo="up")
    assert strokes.skeleton is None
    assert strokes.stereo and "wedge-up" in (strokes.stereo[0].cls or "")


def test_layout_sets_wedge_stereo():
    backend = _backend()
    layout = Pict(backend=backend).layout({"molecules": [{"smiles": "C[C@H](O)Cl"}]}).molecules[0]
    stereos = {b.stereo for b in layout.bonds if b.stereo and b.stereo != "none"}
    assert stereos & {"up", "down"}, (
        f"expected wedge stereo, got {[b.stereo for b in layout.bonds]}"
    )


def test_chiral_molecule_svg_contains_wedge_or_hash():
    backend = _backend()
    svg = render({"molecules": [{"smiles": "C[C@H](O)Cl"}]}, backend=backend)
    assert "<svg" in svg
    assert "bond-wedge" in svg, "expected solid or hashed wedge class in SVG"
    assert "bond-skeleton" in svg


def test_either_single_is_wavy():
    strokes = bond_strokes(0, 0, 20, 0, 1.0, stereo="either")
    assert strokes.skeleton is None
    assert strokes.stereo and "bond-either" in (strokes.stereo[0].cls or "")


def test_either_double_is_crossed():
    strokes = bond_strokes(0, 0, 20, 0, 2.0, stereo="either")
    assert any("either-cross" in (p.cls or "") for p in strokes.stereo)


def _pts(d: str) -> list[tuple[float, float]]:
    return [(float(x), float(y)) for x, y in re.findall(r"[ML]\s+([-\d.]+)\s+([-\d.]+)", d)]


def _on_line(px: float, py: float, x1: float, y1: float, x2: float, y2: float) -> bool:
    cross = abs((x2 - x1) * (py - y1) - (y2 - y1) * (px - x1))
    length = math.hypot(x2 - x1, y2 - y1) or 1.0
    return cross / length < 0.08


def test_one_single_extends_to_the_far_line():
    """Substituent in +y. The single grows past the atom onto the -y line."""
    bonds = [
        DrawnBond(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
        DrawnBond(1, 0, 2, 0.0, 0.0, -10.0, 10.0, 1.0),
    ]
    join_centered_multibonds(bonds)
    assert bonds[1].y1 < -0.5
    assert bonds[1].y1 == pytest.approx(-OFFSET_PX / 2, abs=0.05)
    # The original atom still lies on the extended single.
    assert _on_line(0.0, 0.0, bonds[1].x1, bonds[1].y1, -10.0, 10.0)
    strokes = bond_strokes(0, 0, 20, 0, 2.0, trims=bonds[0].trims)
    for path in strokes.offsets:
        end = _pts(path.d)[0]
        assert _on_line(end[0], end[1], bonds[1].x1, bonds[1].y1, -10.0, 10.0)


def test_two_singles_meet_and_clip_the_double():
    """Two singles stay at the atom; double strokes extend onto those singles."""
    bonds = [
        DrawnBond(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
        DrawnBond(1, 0, 2, 0.0, 0.0, -10.0, 8.0, 1.0),
        DrawnBond(2, 0, 3, 0.0, 0.0, -10.0, -8.0, 1.0),
    ]
    join_centered_multibonds(bonds)
    # Singles are not stub-extended past the atom.
    assert bonds[1].x1 == pytest.approx(0.0)
    assert bonds[1].y1 == pytest.approx(0.0)
    assert bonds[2].x1 == pytest.approx(0.0)
    assert bonds[2].y1 == pytest.approx(0.0)
    assert bonds[0].trims is not None
    # Trims are negative: strokes extend past the atom onto the singles.
    assert all(t < 0.0 for t in bonds[0].trims[0])
    strokes = bond_strokes(0, 0, 20, 0, 2.0, trims=bonds[0].trims)
    for path in strokes.offsets:
        x, y = _pts(path.d)[0]
        on_upper = _on_line(x, y, 0.0, 0.0, -10.0, 8.0)
        on_lower = _on_line(x, y, 0.0, 0.0, -10.0, -8.0)
        assert on_upper or on_lower
        assert x < -0.2  # past the atom, on the single-bond side


def test_two_singles_double_ends_lie_on_singles_acetone():
    """Acetone: each C=O stroke end sits on a methyl single."""
    from xpict.backends import get_backend
    from xpict.contracts.nodes import expand_pict
    from xpict.draw.drawable import normalize_coords
    from xpict.draw.metrics import shared_coord_scale

    doc = expand_pict({"molecules": [{"smiles": "CC(=O)C"}]}).to_legacy()
    layout = get_backend("native").layout(doc.molecules[0])
    coords, _, _ = normalize_coords(layout, scale=shared_coord_scale([layout]))
    bonds = []
    for b in layout.bonds:
        x1, y1 = coords[b.begin]
        x2, y2 = coords[b.end]
        bonds.append(
            DrawnBond(
                b.index,
                b.begin,
                b.end,
                x1,
                y1,
                x2,
                y2,
                b.order,
                begin_labeled=layout.atoms[b.begin].element != "C",
                end_labeled=layout.atoms[b.end].element != "C",
            )
        )
    join_centered_multibonds(bonds)
    dbl = next(b for b in bonds if b.order >= 1.5)
    singles = [b for b in bonds if b.order < 1.5]
    assert dbl.trims is not None
    strokes = bond_strokes(dbl.x1, dbl.y1, dbl.x2, dbl.y2, 2.0, trims=dbl.trims)
    for path in strokes.offsets:
        x, y = _pts(path.d)[0]
        assert any(_on_line(x, y, s.x1, s.y1, s.x2, s.y2) for s in singles), (
            f"end ({x:.2f},{y:.2f}) not on a single"
        )


def test_far_end_single_joins_too():
    """The mitre at the far end uses the same lines as the near end."""
    bonds = [
        DrawnBond(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
        DrawnBond(1, 1, 2, 20.0, 0.0, 30.0, -10.0, 1.0),
    ]
    join_centered_multibonds(bonds)
    # Substituent is -y of the far atom, so the single meets the +y line.
    assert bonds[1].y1 == pytest.approx(OFFSET_PX / 2, abs=0.05)
    assert bonds[1].x1 < 20.0
    strokes = bond_strokes(0, 0, 20, 0, 2.0, trims=bonds[0].trims)
    for path in strokes.offsets:
        end = _pts(path.d)[1]
        assert _on_line(end[0], end[1], bonds[1].x1, bonds[1].y1, 30.0, -10.0)


def test_acute_two_singles_still_land_on_lines():
    """~25° between singles — intersection math must still close the vertex."""
    bonds = [
        DrawnBond(0, 0, 1, 0.0, 0.0, 20.0, 0.0, 2.0),
        DrawnBond(1, 0, 2, 0.0, 0.0, -18.0, 4.0, 1.0),
        DrawnBond(2, 0, 3, 0.0, 0.0, -18.0, -4.0, 1.0),
    ]
    join_centered_multibonds(bonds)
    assert bonds[0].trims is not None
    assert all(t < 0.0 for t in bonds[0].trims[0])
    strokes = bond_strokes(0, 0, 20, 0, 2.0, trims=bonds[0].trims)
    for path in strokes.offsets:
        x, y = _pts(path.d)[0]
        assert _on_line(x, y, 0.0, 0.0, -18.0, 4.0) or _on_line(x, y, 0.0, 0.0, -18.0, -4.0)
        assert x < -0.2


def test_line_intersect_unit_axes():
    from xpict.draw.bonds import line_intersect

    t, s, ix, iy = line_intersect(0, 1, 1, 0, 1, 0, 0, 1)
    assert t == pytest.approx(1.0)
    assert s == pytest.approx(1.0)
    assert ix == pytest.approx(1.0)
    assert iy == pytest.approx(1.0)
    assert line_intersect(0, 0, 1, 0, 0, 1, 1, 0) is None

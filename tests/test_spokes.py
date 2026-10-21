"""Ring-center spokes from CXSMILES ``m:`` (dummy dropped from the spoke graph)."""

from __future__ import annotations

import math

from xpict import mol, render


def _xy(mol, idx: int) -> tuple[float, float]:
    a = next(x for x in mol.atoms if x.index == idx)
    return float(a.x), float(a.y)


def _centroid(mol, idxs) -> tuple[float, float]:
    pts = [_xy(mol, i) for i in idxs]
    n = len(pts)
    return sum(p[0] for p in pts) / n, sum(p[1] for p in pts) / n


def _ring_bonds(mol, endpts):
    s = set(endpts)
    return [b for b in mol.bonds if b.begin in s and b.end in s]


def _mean_ring_bond(mol, endpts) -> float:
    lens = []
    for b in _ring_bonds(mol, endpts):
        p, q = _xy(mol, b.begin), _xy(mol, b.end)
        lens.append(math.hypot(p[0] - q[0], p[1] - q[1]))
    return sum(lens) / len(lens)


def _min_dist_to_ring(mol, pt, endpts) -> float:
    return min(
        _seg_dist(pt, _xy(mol, b.begin), _xy(mol, b.end))
        for b in _ring_bonds(mol, endpts)
    )


def _turn(a: tuple[float, float], b: tuple[float, float], c: tuple[float, float]) -> float:
    ux, uy = b[0] - a[0], b[1] - a[1]
    vx, vy = c[0] - b[0], c[1] - b[1]
    lu = math.hypot(ux, uy)
    lv = math.hypot(vx, vy)
    if lu < 1e-9 or lv < 1e-9:
        return 0.0
    return abs(math.atan2(ux * vy - uy * vx, ux * vx + uy * vy))


def _seg_dist(p, a, b) -> float:
    dx, dy = b[0] - a[0], b[1] - a[1]
    l2 = dx * dx + dy * dy
    if l2 < 1e-18:
        return math.hypot(p[0] - a[0], p[1] - a[1])
    t = max(0.0, min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2))
    return math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy))


def test_cc_star_benzene_spoke_has_chain_kink():
    m = render(mol("CC*.c1ccccc1 |m:2:3.4.5.6.7.8|")).molecule
    assert len(m.spokes) == 1
    sp = m.spokes[0]
    assert sp.atom == 1
    cx, cy = _centroid(m, sp.endpts)
    attach = _xy(m, sp.atom)
    methyl = _xy(m, 0)
    turn = _turn((cx, cy), attach, methyl)
    assert 0.7 < turn < 1.5, turn
    dummy = next(a for a in m.atoms if a.index == sp.dummy)
    assert dummy.label in (None, "")


def test_star_cc_ethylbenzene_spoke_has_kink():
    m = render(mol("c1ccccc1CC.*CC |m:8:0.1.2.3.4.5|")).molecule
    assert len(m.spokes) == 1
    sp = m.spokes[0]
    cx, cy = _centroid(m, sp.endpts)
    attach = _xy(m, sp.atom)
    methyl = _xy(m, 10)
    turn = _turn((cx, cy), attach, methyl)
    assert 0.7 < turn < 1.5, turn


def test_o_star_sits_one_bond_outside_ring_and_keeps_aromatic_circle():
    from xpict.rendered import to_svg

    rendered = render(mol("O*.c1ccccc1 |m:1:2.3.4.5.6.7|"))
    m = rendered.molecule
    assert len(m.spokes) == 1
    sp = m.spokes[0]
    bond = _mean_ring_bond(m, sp.endpts)
    attach = _xy(m, sp.atom)
    d_ring = _min_dist_to_ring(m, attach, sp.endpts)
    assert 0.7 * bond < d_ring < 1.4 * bond, d_ring
    cx, cy = _centroid(m, sp.endpts)
    from_c = math.hypot(attach[0] - cx, attach[1] - cy)
    assert from_c > 1.4 * bond, from_c
    dummy = _xy(m, sp.dummy)
    assert math.hypot(dummy[0] - cx, dummy[1] - cy) < 0.2 * bond
    svg = to_svg(rendered.scene)
    assert "aromatic" in svg
    assert 'r="0"' not in svg and "r='0'" not in svg


def test_cl_star_on_ethylbenzene_uses_ring_not_ethyl():
    m = render(mol("Cl*.c1ccc(CC)cc1 |m:1:2.3.4.5.6.7|")).molecule
    assert len(m.spokes) == 1
    sp = m.spokes[0]
    assert sp.atom == 0
    assert sorted(sp.endpts) == [2, 3, 4, 5, 8, 9]
    cx, cy = _centroid(m, sp.endpts)
    dummy = _xy(m, sp.dummy)
    attach = _xy(m, sp.atom)
    bond = _mean_ring_bond(m, sp.endpts)
    assert math.hypot(dummy[0] - cx, dummy[1] - cy) < 0.2 * bond
    d_ring = _min_dist_to_ring(m, attach, sp.endpts)
    assert 0.7 * bond < d_ring < 1.4 * bond, d_ring


def test_two_spokes_on_benzene():
    m = render(mol("CC*.CC*.c1ccccc1 |m:2:6.7.8.9.10.11,5:6.7.8.9.10.11|")).molecule
    assert len(m.spokes) == 2
    cx, cy = _centroid(m, m.spokes[0].endpts)
    a0 = _xy(m, m.spokes[0].atom)
    a1 = _xy(m, m.spokes[1].atom)
    t0 = math.atan2(a0[1] - cy, a0[0] - cx)
    t1 = math.atan2(a1[1] - cy, a1[0] - cx)
    d = abs((t0 - t1 + math.pi) % (2 * math.pi) - math.pi)
    assert d > 0.8

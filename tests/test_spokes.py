"""Ring-center spokes from CXSMILES ``m:`` (dummy dropped from the spoke graph)."""

from __future__ import annotations

import math

from xpict import mol, render


def _xy(mol, idx: int) -> tuple[float, float]:
    a = next(x for x in mol.atoms if x.index == idx)
    return float(a.x), float(a.y)


def _turn(a: tuple[float, float], b: tuple[float, float], c: tuple[float, float]) -> float:
    ux, uy = b[0] - a[0], b[1] - a[1]
    vx, vy = c[0] - b[0], c[1] - b[1]
    lu = math.hypot(ux, uy)
    lv = math.hypot(vx, vy)
    if lu < 1e-9 or lv < 1e-9:
        return 0.0
    return abs(math.atan2(ux * vy - uy * vx, ux * vx + uy * vy))


def test_cc_star_benzene_spoke_has_chain_kink():
    m = render(mol("CC*.c1ccccc1 |m:2:3.4.5.6.7.8|")).molecule
    assert len(m.spokes) == 1
    sp = m.spokes[0]
    assert sp.atom == 1
    pts = [_xy(m, i) for i in sp.endpts]
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
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
    pts = [_xy(m, i) for i in sp.endpts]
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
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
    pts = [_xy(m, i) for i in sp.endpts]
    endpt_set = set(sp.endpts)

    def _bond_len(b) -> float:
        p, q = _xy(m, b.begin), _xy(m, b.end)
        return math.hypot(p[0] - q[0], p[1] - q[1])

    ring_lens = [
        _bond_len(b) for b in m.bonds if b.begin in endpt_set and b.end in endpt_set
    ]
    bond = sum(ring_lens) / len(ring_lens)
    attach = _xy(m, sp.atom)
    d_ring = min(
        _seg_dist(attach, _xy(m, b.begin), _xy(m, b.end))
        for b in m.bonds
        if b.begin in endpt_set and b.end in endpt_set
    )
    assert 0.7 * bond < d_ring < 1.4 * bond, d_ring
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    from_c = math.hypot(attach[0] - cx, attach[1] - cy)
    assert from_c > 1.4 * bond, from_c
    dummy = _xy(m, sp.dummy)
    assert math.hypot(dummy[0] - cx, dummy[1] - cy) < 0.2 * bond
    svg = to_svg(rendered.scene)
    assert "aromatic" in svg
    assert 'r="0"' not in svg and "r='0'" not in svg


def _seg_dist(p, a, b) -> float:
    dx, dy = b[0] - a[0], b[1] - a[1]
    l2 = dx * dx + dy * dy
    if l2 < 1e-18:
        return math.hypot(p[0] - a[0], p[1] - a[1])
    t = max(0.0, min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2))
    return math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy))


def test_cl_star_on_ethylbenzene_uses_ring_not_ethyl():
    m = render(mol("Cl*.c1ccc(CC)cc1 |m:1:2.3.4.5.6.7|")).molecule
    assert len(m.spokes) == 1
    sp = m.spokes[0]
    assert sp.atom == 0
    assert sorted(sp.endpts) == [2, 3, 4, 5, 8, 9]
    pts = [_xy(m, i) for i in sp.endpts]
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    dummy = _xy(m, sp.dummy)
    attach = _xy(m, sp.atom)
    endpt_set = set(sp.endpts)
    ring_lens = []
    for b in m.bonds:
        if b.begin in endpt_set and b.end in endpt_set:
            p, q = _xy(m, b.begin), _xy(m, b.end)
            ring_lens.append(math.hypot(p[0] - q[0], p[1] - q[1]))
    bond = sum(ring_lens) / len(ring_lens)
    assert math.hypot(dummy[0] - cx, dummy[1] - cy) < 0.2 * bond
    d_ring = min(
        _seg_dist(attach, _xy(m, b.begin), _xy(m, b.end))
        for b in m.bonds
        if b.begin in endpt_set and b.end in endpt_set
    )
    assert 0.7 * bond < d_ring < 1.4 * bond, d_ring


def test_two_spokes_on_benzene():
    m = render(mol("CC*.CC*.c1ccccc1 |m:2:6.7.8.9.10.11,5:6.7.8.9.10.11|")).molecule
    assert len(m.spokes) == 2
    pts = [_xy(m, i) for i in m.spokes[0].endpts]
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    a0 = _xy(m, m.spokes[0].atom)
    a1 = _xy(m, m.spokes[1].atom)
    t0 = math.atan2(a0[1] - cy, a0[0] - cx)
    t1 = math.atan2(a1[1] - cy, a1[0] - cx)
    d = abs((t0 - t1 + math.pi) % (2 * math.pi) - math.pi)
    assert d > 0.8

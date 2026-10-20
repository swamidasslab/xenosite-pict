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


def test_cc_star_benzene_spoke_is_collinear():
    m = render(mol("CC*.c1ccccc1 |m:2:3.4.5.6.7.8|")).molecule
    assert len(m.spokes) == 1
    sp = m.spokes[0]
    assert sp.atom == 1
    pts = [_xy(m, i) for i in sp.endpts]
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    attach = _xy(m, sp.atom)
    methyl = _xy(m, 0)
    assert _turn((cx, cy), attach, methyl) < 0.15
    dummy = next(a for a in m.atoms if a.index == sp.dummy)
    assert dummy.label in (None, "")


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

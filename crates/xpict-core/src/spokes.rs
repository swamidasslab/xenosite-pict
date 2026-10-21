//! Ring-center spoke detection and rigid placement (CX ``m:``).
//!
//! The SMILES dummy ``*`` and its bond are **dropped from the spoke graph**.
//! CoordGen therefore never sees ``*`` as a neighbor of the attachment atom,
//! so ``CC*`` stays a straight two-bond stem (no extra kink at a dummy vertex).
//! A silent dummy sits at the centroid; the painted stem runs from there to
//! the substituent (one bond past the ring).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::cxsmiles::Multicenter;
use crate::rings::find_relevant_rings_from_adj;

/// One ring-center substituent after dropping the dummy from the spoke graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpokeComp {
    pub dummy: i32,
    /// First real atom of the substituent (was bonded to ``*``).
    pub atom: i32,
    /// Substituent atoms including [`Self::atom`], excluding the dummy.
    pub atoms: Vec<i32>,
    pub endpts: Vec<i32>,
}

/// Detect CX ``m:`` groups whose endpoints lie on a ring (fusion allowed).
pub fn detect_ring_spokes(
    n: usize,
    bonds: &[(i32, i32)],
    groups: &[Multicenter],
) -> Vec<SpokeComp> {
    if n == 0 || groups.is_empty() {
        return Vec::new();
    }
    let adj = adjacency(n, bonds);
    let mut out = Vec::new();
    let mut taken: HashSet<i32> = HashSet::new();
    for g in groups {
        let dummy = g.dummy;
        if dummy < 0 || dummy as usize >= n {
            continue;
        }
        if g.endpts.len() < 3 {
            continue;
        }
        if g.endpts
            .iter()
            .any(|&a| a < 0 || a as usize >= n || a == dummy)
        {
            continue;
        }
        if !endpts_are_ring_system(&adj, bonds, dummy, &g.endpts) {
            continue;
        }
        let blocked: HashSet<i32> = g.endpts.iter().copied().collect();
        let mut spoke = bfs_component(&adj, dummy, &blocked);
        spoke.sort_unstable();
        spoke.dedup();
        let attachment = adj
            .get(dummy as usize)
            .into_iter()
            .flatten()
            .copied()
            .find(|&nb| nb != dummy && !blocked.contains(&nb));
        let Some(atom) = attachment else {
            continue;
        };
        let atoms: Vec<i32> = spoke.into_iter().filter(|&a| a != dummy).collect();
        if atoms.is_empty() || !atoms.contains(&atom) {
            continue;
        }
        if atoms.iter().any(|a| taken.contains(a)) || taken.contains(&dummy) {
            continue;
        }
        for &a in &atoms {
            taken.insert(a);
        }
        taken.insert(dummy);
        out.push(SpokeComp {
            dummy,
            atom,
            atoms,
            endpts: g.endpts.clone(),
        });
    }
    out
}

fn adjacency(n: usize, bonds: &[(i32, i32)]) -> Vec<Vec<i32>> {
    let mut adj = vec![Vec::new(); n];
    for &(a, b) in bonds {
        if a < 0 || b < 0 {
            continue;
        }
        let (au, bu) = (a as usize, b as usize);
        if au >= n || bu >= n || au == bu {
            continue;
        }
        adj[au].push(b);
        adj[bu].push(a);
    }
    adj
}

/// Endpoints are a ring (or fused/bridged ring system).
///
/// Fusion / bridgehead atoms have degree >2 in the induced subgraph, so we
/// do **not** require 2-regular cycles. Each endpoint must be cyclic, and
/// the induced subgraph must be connected. Dummy–endpoint bonds are ignored
/// so a haptic-style dummy cannot fake a ring.
fn endpts_are_ring_system(
    adj: &[Vec<i32>],
    bonds: &[(i32, i32)],
    dummy: i32,
    endpts: &[i32],
) -> bool {
    if endpts.len() < 3 {
        return false;
    }
    let set: HashSet<i32> = endpts.iter().copied().collect();
    if set.len() != endpts.len() {
        return false;
    }
    if !induced_connected(adj, endpts) {
        return false;
    }
    let map = adj_map_without_dummy(adj, dummy);
    let rest_bonds = bonds
        .iter()
        .copied()
        .filter(|&(a, b)| a != dummy && b != dummy);
    let rings = find_relevant_rings_from_adj(&map, rest_bonds, 32);
    if rings.is_empty() {
        return false;
    }
    if rings
        .iter()
        .any(|r| endpts.iter().all(|a| r.atoms.contains(a)))
    {
        return true;
    }
    endpts
        .iter()
        .all(|&a| rings.iter().any(|r| r.atoms.contains(&a)))
}

fn induced_connected(adj: &[Vec<i32>], endpts: &[i32]) -> bool {
    let set: HashSet<i32> = endpts.iter().copied().collect();
    let start = endpts[0];
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    q.push_back(start);
    seen.insert(start);
    while let Some(cur) = q.pop_front() {
        let Some(nbs) = adj.get(cur as usize) else {
            continue;
        };
        for &nb in nbs {
            if !set.contains(&nb) || !seen.insert(nb) {
                continue;
            }
            q.push_back(nb);
        }
    }
    seen.len() == set.len()
}

fn adj_map_without_dummy(adj: &[Vec<i32>], dummy: i32) -> HashMap<i32, Vec<i32>> {
    adj.iter()
        .enumerate()
        .filter(|(i, _)| *i as i32 != dummy)
        .map(|(i, nbs)| {
            let filtered: Vec<i32> = nbs.iter().copied().filter(|&n| n != dummy).collect();
            (i as i32, filtered)
        })
        .collect()
}

fn bfs_component(adj: &[Vec<i32>], start: i32, blocked: &HashSet<i32>) -> Vec<i32> {
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    q.push_back(start);
    seen.insert(start);
    while let Some(cur) = q.pop_front() {
        let Some(nbs) = adj.get(cur as usize) else {
            continue;
        };
        for &nb in nbs {
            if blocked.contains(&nb) || !seen.insert(nb) {
                continue;
            }
            q.push_back(nb);
        }
    }
    seen.into_iter().collect()
}

/// First substituent neighbor of ``attach`` in ``spoke_atoms`` (not dummy).
pub fn first_spoke_neighbor(
    bonds: &[(i32, i32)],
    attach: i32,
    spoke: &[i32],
    dummy: i32,
) -> Option<i32> {
    let set: HashSet<i32> = spoke.iter().copied().collect();
    for &(a, b) in bonds {
        let other = if a == attach {
            b
        } else if b == attach {
            a
        } else {
            continue;
        };
        if other != dummy && set.contains(&other) {
            return Some(other);
        }
    }
    None
}

/// Outward angles of ring-bond perpendicular bisectors (centroid → midpoint).
pub fn bisector_angles(
    centroid: (f64, f64),
    ring_xy: &[(i32, f64, f64)],
    bonds: &[(i32, i32)],
) -> Vec<f64> {
    let mut out = Vec::new();
    let set: HashSet<i32> = ring_xy.iter().map(|&(i, _, _)| i).collect();
    let xy: std::collections::HashMap<i32, (f64, f64)> =
        ring_xy.iter().map(|&(i, x, y)| (i, (x, y))).collect();
    for &(a, b) in bonds {
        if !set.contains(&a) || !set.contains(&b) {
            continue;
        }
        let Some(&(ax, ay)) = xy.get(&a) else {
            continue;
        };
        let Some(&(bx, by)) = xy.get(&b) else {
            continue;
        };
        let mx = 0.5 * (ax + bx);
        let my = 0.5 * (ay + by);
        let dx = mx - centroid.0;
        let dy = my - centroid.1;
        if dx.hypot(dy) < 1e-9 {
            continue;
        }
        out.push(dy.atan2(dx));
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup_by(|a, b| ang_diff(*a, *b) < 1e-6);
    out
}

pub fn ang_diff(a: f64, b: f64) -> f64 {
    let mut d = (a - b).rem_euclid(std::f64::consts::TAU);
    if d > std::f64::consts::PI {
        d = std::f64::consts::TAU - d;
    }
    d
}

/// Distance from the ring centroid along ``angle`` to the ring perimeter.
///
/// Hits the first ring-bond segment the ray meets (a bond-gap exit). Falls
/// back to inradius if the ray misses every segment.
pub fn ring_exit_along_ray(
    centroid: (f64, f64),
    angle: f64,
    ring_xy: &[(i32, f64, f64)],
    bonds: &[(i32, i32)],
) -> f64 {
    let u = (angle.cos(), angle.sin());
    let set: HashSet<i32> = ring_xy.iter().map(|&(i, _, _)| i).collect();
    let xy: HashMap<i32, (f64, f64)> = ring_xy.iter().map(|&(i, x, y)| (i, (x, y))).collect();
    let mut hit = f64::INFINITY;
    for &(a, b) in bonds {
        if !set.contains(&a) || !set.contains(&b) {
            continue;
        }
        let Some(&pa) = xy.get(&a) else {
            continue;
        };
        let Some(&pb) = xy.get(&b) else {
            continue;
        };
        if let Some(t) = ray_segment_t(centroid, u, pa, pb) {
            if t > 1e-6 {
                hit = hit.min(t);
            }
        }
    }
    if hit.is_finite() {
        return hit;
    }
    let mut inr = f64::INFINITY;
    for &(a, b) in bonds {
        if !set.contains(&a) || !set.contains(&b) {
            continue;
        }
        let Some(&(x1, y1)) = xy.get(&a) else {
            continue;
        };
        let Some(&(x2, y2)) = xy.get(&b) else {
            continue;
        };
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = dx.hypot(dy).max(1e-9);
        let d = ((centroid.0 - x1) * dy - (centroid.1 - y1) * dx).abs() / len;
        inr = inr.min(d);
    }
    if inr.is_finite() {
        inr
    } else {
        ring_xy
            .iter()
            .map(|&(_, x, y)| (x - centroid.0).hypot(y - centroid.1))
            .sum::<f64>()
            / ring_xy.len().max(1) as f64
    }
}

fn ray_segment_t(origin: (f64, f64), dir: (f64, f64), a: (f64, f64), b: (f64, f64)) -> Option<f64> {
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let cross = dir.0 * vy - dir.1 * vx;
    if cross.abs() < 1e-12 {
        return None;
    }
    let acx = a.0 - origin.0;
    let acy = a.1 - origin.1;
    let t = (acx * vy - acy * vx) / cross;
    let s = (acx * dir.1 - acy * dir.0) / cross;
    if t > 0.0 && (0.0..=1.0).contains(&s) {
        Some(t)
    } else {
        None
    }
}

/// Stem length: ring perimeter along the ray, plus one house bond — not from
/// the centroid (too close) and not an extra bond past the ring (too far).
pub fn spoke_stem_len(
    centroid: (f64, f64),
    angle: f64,
    ring_xy: &[(i32, f64, f64)],
    bonds: &[(i32, i32)],
    bond_len: f64,
) -> f64 {
    ring_exit_along_ray(centroid, angle, ring_xy, bonds) + bond_len.max(0.0)
}

pub fn point_segment_dist(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let l2 = dx * dx + dy * dy;
    if l2 < 1e-18 {
        return (px - x0).hypot(py - y0);
    }
    let t = ((px - x0) * dx + (py - y0) * dy) / l2;
    let t = t.clamp(0.0, 1.0);
    (px - (x0 + t * dx)).hypot(py - (y0 + t * dy))
}

/// Rotate/translate ``local`` so ``attach`` sits at ``centroid + L·u(θ)`` and
/// the first spoke bond is **parallel** to the stem (same sense, outward).
pub fn place_spoke(
    local: &[(f64, f64)],
    attach_idx: usize,
    neighbor_idx: Option<usize>,
    centroid: (f64, f64),
    angle: f64,
    stem_len: f64,
) -> Vec<(f64, f64)> {
    if local.is_empty() || attach_idx >= local.len() {
        return local.to_vec();
    }
    let u = (angle.cos(), angle.sin());
    let target = (centroid.0 + stem_len * u.0, centroid.1 + stem_len * u.1);
    let a = local[attach_idx];
    let (c, s) = if let Some(ni) = neighbor_idx.filter(|&i| i < local.len() && i != attach_idx) {
        let n = local[ni];
        let lv = (n.0 - a.0, n.1 - a.1);
        let local_ang = lv.1.atan2(lv.0);
        let rot = angle - local_ang;
        (rot.cos(), rot.sin())
    } else {
        (1.0, 0.0)
    };
    local
        .iter()
        .map(|&(x, y)| {
            let dx = x - a.0;
            let dy = y - a.1;
            (target.0 + c * dx - s * dy, target.1 + s * dx + c * dy)
        })
        .collect()
}

/// Interior direction change at ``b`` along ``a → b → c`` (0 = collinear).
pub fn kink_turn(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let u = (b.0 - a.0, b.1 - a.1);
    let v = (c.0 - b.0, c.1 - b.1);
    let lu = u.0.hypot(u.1);
    let lv = v.0.hypot(v.1);
    if lu < 1e-9 || lv < 1e-9 {
        return 0.0;
    }
    let dot = (u.0 * v.0 + u.1 * v.1) / (lu * lv);
    let cross = u.0 * v.1 - u.1 * v.0;
    cross.atan2(dot).abs()
}

const HARD: f64 = 1.0e6;

/// Energy of a stem angle: hard no-cross of ring atoms, soft snap to a bisector.
pub fn spoke_angle_energy(
    theta: f64,
    centroid: (f64, f64),
    bond_len: f64,
    ring_xy: &[(i32, f64, f64)],
    bonds: &[(i32, i32)],
    disk: f64,
    bisectors: &[f64],
    taken: &[f64],
) -> f64 {
    let ux = theta.cos();
    let uy = theta.sin();
    let stem = spoke_stem_len(centroid, theta, ring_xy, bonds, bond_len);
    let ax = centroid.0 + stem * ux;
    let ay = centroid.1 + stem * uy;
    let mut e = 0.0;
    for &(_, x, y) in ring_xy {
        let d = point_segment_dist(x, y, centroid.0, centroid.1, ax, ay);
        if d < disk {
            let g = disk - d;
            e += HARD * g * g;
        }
    }
    if let Some(best) = bisectors
        .iter()
        .map(|&b| ang_diff(theta, b))
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        e += 8.0 * best * best;
    }
    for &t in taken {
        let gap = ang_diff(theta, t);
        let min_gap = std::f64::consts::PI / 3.0;
        if gap < min_gap {
            let g = min_gap - gap;
            e += 40.0 * g * g;
        }
    }
    e
}

/// Greedy bisector slot, then 1-D refine (rigid rotation around the centroid).
pub fn greedy_spoke_angle(
    centroid: (f64, f64),
    bond_len: f64,
    ring_xy: &[(i32, f64, f64)],
    bonds: &[(i32, i32)],
    disk: f64,
    bisectors: &[f64],
    taken: &[f64],
) -> f64 {
    let mut best_th = bisectors.first().copied().unwrap_or(0.0);
    let mut best_e = f64::INFINITY;
    let candidates: Vec<f64> = if bisectors.is_empty() {
        (0..12)
            .map(|i| i as f64 * std::f64::consts::TAU / 12.0)
            .collect()
    } else {
        bisectors.to_vec()
    };
    for &th in &candidates {
        let e = spoke_angle_energy(
            th, centroid, bond_len, ring_xy, bonds, disk, bisectors, taken,
        );
        if e < best_e {
            best_e = e;
            best_th = th;
        }
    }
    let window = if bisectors.len() >= 2 {
        std::f64::consts::PI / bisectors.len() as f64
    } else {
        std::f64::consts::PI / 6.0
    };
    let steps = 21;
    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        let th = best_th - window + 2.0 * window * t;
        let e = spoke_angle_energy(
            th, centroid, bond_len, ring_xy, bonds, disk, bisectors, taken,
        );
        if e < best_e {
            best_e = e;
            best_th = th;
        }
    }
    best_th
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cxsmiles::cx_multicenters;
    use std::collections::HashSet;

    #[test]
    fn detects_cc_star_on_benzene() {
        // C0-C1-*2  .  c3..c8
        let bonds = [
            (0, 1),
            (1, 2),
            (3, 4),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 8),
            (8, 3),
        ];
        let g = cx_multicenters("CC*.c1ccccc1 |m:2:3.4.5.6.7.8|");
        let comps = detect_ring_spokes(9, &bonds, &g);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].dummy, 2);
        assert_eq!(comps[0].atom, 1);
        let mut atoms = comps[0].atoms.clone();
        atoms.sort_unstable();
        assert_eq!(atoms, vec![0, 1]);
    }

    #[test]
    fn fused_ring_endpts_allow_degree_gt_two() {
        // Naphthalene `c1ccc2ccccc2c1` (fusion atoms degree 3) + `CC*`.
        // Atoms: C0-C1-*2, then 3..12 naphthalene. Fusion 6 and 11.
        let mut bonds = vec![(0, 1), (1, 2)];
        bonds.extend([
            (3, 4),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 8),
            (8, 9),
            (9, 10),
            (10, 11),
            (11, 6),
            (11, 12),
            (12, 3),
        ]);
        let g = cx_multicenters("CC*.c1ccc2ccccc2c1 |m:2:3.4.5.6.7.8.9.10.11.12|");
        let comps = detect_ring_spokes(13, &bonds, &g);
        assert_eq!(
            comps.len(),
            1,
            "fusion atoms are cyclic, not a 2-regular subset"
        );
        assert_eq!(comps[0].atom, 1);
        let set: HashSet<i32> = g[0].endpts.iter().copied().collect();
        let deg = |a: i32| {
            bonds
                .iter()
                .filter(|&&(x, y)| (x == a && set.contains(&y)) || (y == a && set.contains(&x)))
                .count()
        };
        assert!(
            deg(6) >= 3 && deg(11) >= 3,
            "sanity: fusion degree in subset"
        );
    }

    #[test]
    fn chain_endpts_are_not_a_ring_spoke() {
        let bonds = [(0, 1), (1, 2), (3, 4), (4, 5), (5, 6)];
        let g = cx_multicenters("CC*.CCCC |m:2:3.4.5.6|");
        assert!(detect_ring_spokes(7, &bonds, &g).is_empty());
    }

    #[test]
    fn hexagon_stem_is_inradius_plus_one_bond() {
        let r3 = 3.0_f64.sqrt();
        let ring = [
            (0, 1.0, 0.0),
            (1, 0.5, r3 / 2.0),
            (2, -0.5, r3 / 2.0),
            (3, -1.0, 0.0),
            (4, -0.5, -r3 / 2.0),
            (5, 0.5, -r3 / 2.0),
        ];
        let bonds = [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0)];
        let ang = std::f64::consts::PI / 6.0;
        let exit = ring_exit_along_ray((0.0, 0.0), ang, &ring, &bonds);
        assert!(
            (exit - r3 / 2.0).abs() < 1e-9,
            "bisector should hit inradius, exit={exit}"
        );
        let stem = spoke_stem_len((0.0, 0.0), ang, &ring, &bonds, 1.0);
        assert!((stem - (exit + 1.0)).abs() < 1e-9);
    }

    #[test]
    fn place_spoke_keeps_first_bond_parallel() {
        let local = [(0.0, 0.0), (1.5, 0.0)];
        let placed = place_spoke(
            &local,
            0,
            Some(1),
            (0.0, 0.0),
            std::f64::consts::FRAC_PI_2,
            1.0,
        );
        let attach = placed[0];
        let next = placed[1];
        let stem = (attach.0, attach.1);
        let bond = (next.0 - attach.0, next.1 - attach.1);
        let cross = stem.0 * bond.1 - stem.1 * bond.0;
        assert!(
            cross.abs() < 1e-9,
            "stem not parallel to first spoke bond: stem={stem:?} bond={bond:?}"
        );
        assert!(
            attach.1 > 0.5,
            "attachment should sit on +y stem, got {attach:?}"
        );
        assert!(kink_turn((0.0, 0.0), attach, next) < 1e-6);
    }

    #[test]
    fn two_multicenters_on_same_ring() {
        let bonds = [
            (0, 1),
            (1, 2),
            (3, 4),
            (4, 5),
            (6, 7),
            (7, 8),
            (8, 9),
            (9, 10),
            (10, 11),
            (11, 6),
        ];
        let g = cx_multicenters("CC*.CC*.c1ccccc1 |m:2:6.7.8.9.10.11,5:6.7.8.9.10.11|");
        let comps = detect_ring_spokes(12, &bonds, &g);
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[0].dummy, 2);
        assert_eq!(comps[1].dummy, 5);
    }
}

//! Practical SSSR and ring-interior normals for depiction.
//!
//! Layout backends own coordinates. This module only supplies drawing helpers:
//! which bonds sit in small rings, and unit normals toward the ring centroid
//! so Kekulé offsets stay short and inside (not the acyclic extend-onto-singles
//! path). Python host draw stack removed; this module is SoT.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::bonds::is_aromatic_order;
use crate::scene::{AtomIn, MoleculeIn};

/// Fraction of ring inradius (min centroid→edge distance) for aromatic circles.
///
/// Inside the skeleton with clear margin so the circle does not crowd bond ink.
pub const AROMATIC_CIRCLE_FRAC: f64 = 0.60;

/// One ring as an ordered atom cycle (atom indices).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ring {
    pub atoms: Vec<i32>,
}

impl Ring {
    pub fn size(&self) -> usize {
        self.atoms.len()
    }

    pub fn bonds(&self) -> HashSet<(i32, i32)> {
        let n = self.atoms.len();
        let mut out = HashSet::with_capacity(n);
        for i in 0..n {
            out.insert(bond_key(self.atoms[i], self.atoms[(i + 1) % n]));
        }
        out
    }
}

fn bond_key(a: i32, b: i32) -> (i32, i32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn adjacency(mol: &MoleculeIn) -> HashMap<i32, Vec<i32>> {
    let mut adj: HashMap<i32, Vec<i32>> = mol.atoms.iter().map(|a| (a.index, Vec::new())).collect();
    for b in &mol.bonds {
        adj.entry(b.begin).or_default().push(b.end);
        adj.entry(b.end).or_default().push(b.begin);
    }
    adj
}

/// Shortest cycle containing edge `u`–`v` (remove the edge, BFS `u`→`v`).
fn shortest_cycle_through_bond(
    adj: &HashMap<i32, Vec<i32>>,
    u: i32,
    v: i32,
    max_size: usize,
) -> Option<Vec<i32>> {
    let mut prev: HashMap<i32, Option<i32>> = HashMap::new();
    prev.insert(u, None);
    let mut q = VecDeque::new();
    q.push_back(u);
    let mut found = false;

    while let Some(cur) = q.pop_front() {
        if found {
            break;
        }
        let mut depth = 0usize;
        let mut t = Some(cur);
        while let Some(node) = t {
            if node == u {
                break;
            }
            depth += 1;
            t = prev.get(&node).copied().flatten();
        }
        if depth > max_size.saturating_sub(1) {
            continue;
        }
        let Some(neighbors) = adj.get(&cur) else {
            continue;
        };
        for &nxt in neighbors {
            if (cur == u && nxt == v) || (cur == v && nxt == u) {
                continue;
            }
            if nxt == v && cur != u {
                prev.insert(v, Some(cur));
                found = true;
                break;
            }
            if prev.contains_key(&nxt) {
                continue;
            }
            prev.insert(nxt, Some(cur));
            q.push_back(nxt);
        }
    }

    if !found || !prev.contains_key(&v) {
        return None;
    }
    let mut path = vec![v];
    let mut cur = Some(v);
    while cur != Some(u) {
        cur = prev.get(&cur?)?.as_ref().copied();
        let node = cur?;
        path.push(node);
        if path.len() > max_size {
            return None;
        }
    }
    path.reverse();
    if path.len() < 3 || path.len() > max_size {
        return None;
    }
    Some(path)
}

/// Cap on alternate shortest paths through one bond (cages, fused systems).
const MAX_SHORTEST_PATHS_PER_BOND: usize = 32;

/// All shortest `u → v` paths that do not use the edge `u`–`v`.
fn all_shortest_paths_avoiding_edge(
    adj: &HashMap<i32, Vec<i32>>,
    u: i32,
    v: i32,
    max_path_edges: usize,
) -> Vec<Vec<i32>> {
    if max_path_edges == 0 {
        return Vec::new();
    }
    let mut dist: HashMap<i32, usize> = HashMap::new();
    let mut parents: HashMap<i32, Vec<i32>> = HashMap::new();
    dist.insert(u, 0);
    let mut q = VecDeque::new();
    q.push_back(u);
    while let Some(cur) = q.pop_front() {
        let d = dist[&cur];
        if d >= max_path_edges {
            continue;
        }
        if let Some(&dv) = dist.get(&v) {
            if d >= dv {
                continue;
            }
        }
        let Some(neighbors) = adj.get(&cur) else {
            continue;
        };
        for &nxt in neighbors {
            if (cur == u && nxt == v) || (cur == v && nxt == u) {
                continue;
            }
            match dist.get(&nxt).copied() {
                None => {
                    dist.insert(nxt, d + 1);
                    parents.insert(nxt, vec![cur]);
                    q.push_back(nxt);
                }
                Some(dn) if dn == d + 1 => {
                    parents.entry(nxt).or_default().push(cur);
                }
                _ => {}
            }
        }
    }
    let Some(&dv) = dist.get(&v) else {
        return Vec::new();
    };
    if dv == 0 || dv > max_path_edges {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut path = Vec::new();
    fn walk(
        cur: i32,
        start: i32,
        parents: &HashMap<i32, Vec<i32>>,
        path: &mut Vec<i32>,
        out: &mut Vec<Vec<i32>>,
        cap: usize,
    ) {
        if out.len() >= cap {
            return;
        }
        path.push(cur);
        if cur == start {
            let mut p = path.clone();
            p.reverse();
            out.push(p);
        } else if let Some(ps) = parents.get(&cur) {
            for &p in ps {
                walk(p, start, parents, path, out, cap);
            }
        }
        path.pop();
    }
    walk(
        v,
        u,
        &parents,
        &mut path,
        &mut out,
        MAX_SHORTEST_PATHS_PER_BOND,
    );
    out
}

fn canonicalize_cycle(atoms: &[i32]) -> Vec<i32> {
    let n = atoms.len();
    if n == 0 {
        return Vec::new();
    }
    let start = (0..n).min_by_key(|&i| atoms[i]).expect("nonempty");
    let next = atoms[(start + 1) % n];
    let prev = atoms[(start + n - 1) % n];
    let mut out = Vec::with_capacity(n);
    if next <= prev {
        for k in 0..n {
            out.push(atoms[(start + k) % n]);
        }
    } else {
        for k in 0..n {
            out.push(atoms[(start + n - k) % n]);
        }
    }
    out
}

/// Relevant Rings: every distinct smallest cycle through each bond.
///
/// This is the union of all SSSR bases (CDK/Vismara relevant cycles), not one
/// greedy SSSR cover. Cubane keeps all six faces; SSSR keeps five.
/// `max_size` is inclusive.
pub fn find_relevant_rings_from_adj(
    adj: &HashMap<i32, Vec<i32>>,
    bonds: impl IntoIterator<Item = (i32, i32)>,
    max_size: usize,
) -> Vec<Ring> {
    if max_size < 3 {
        return Vec::new();
    }
    let mut by_bonds: HashMap<Vec<(i32, i32)>, Ring> = HashMap::new();
    for (u, v) in bonds {
        for path in all_shortest_paths_avoiding_edge(adj, u, v, max_size.saturating_sub(1)) {
            if path.len() < 2 || path.len() > max_size {
                continue;
            }
            let ring = Ring {
                atoms: canonicalize_cycle(&path),
            };
            if ring.size() < 3 || ring.size() > max_size {
                continue;
            }
            let mut keys: Vec<_> = ring.bonds().into_iter().collect();
            keys.sort_unstable();
            by_bonds.entry(keys).or_insert(ring);
        }
    }
    let mut rings: Vec<Ring> = by_bonds.into_values().collect();
    rings.sort_by(|a, b| a.size().cmp(&b.size()).then_with(|| a.atoms.cmp(&b.atoms)));
    rings
}

/// Practical SSSR from adjacency + undirected bonds.
///
/// Unique shortest cycles through each bond, then a greedy cover that prefers
/// smaller rings. Depiction uses this (aromatic circles, interiors). Align
/// uses [`find_relevant_rings_from_adj`] instead. `max_size` is inclusive.
pub fn find_sssr_from_adj(
    adj: &HashMap<i32, Vec<i32>>,
    bonds: impl IntoIterator<Item = (i32, i32)>,
    max_size: usize,
) -> Vec<Ring> {
    let mut by_bonds: HashMap<Vec<(i32, i32)>, Ring> = HashMap::new();
    for (u, v) in bonds {
        let Some(cyc) = shortest_cycle_through_bond(adj, u, v, max_size) else {
            continue;
        };
        let ring = Ring { atoms: cyc };
        let mut keys: Vec<_> = ring.bonds().into_iter().collect();
        keys.sort_unstable();
        match by_bonds.get(&keys) {
            Some(prev) if prev.size() <= ring.size() => {}
            _ => {
                by_bonds.insert(keys, ring);
            }
        }
    }
    let mut rings: Vec<Ring> = by_bonds.into_values().collect();
    rings.sort_by(|a, b| a.size().cmp(&b.size()).then_with(|| a.atoms.cmp(&b.atoms)));
    // Greedy cover: prefer smaller rings; keep a ring if it adds a new bond.
    let mut kept = Vec::new();
    let mut covered: HashSet<(i32, i32)> = HashSet::new();
    for ring in &rings {
        let bonds = ring.bonds();
        if bonds.iter().any(|b| !covered.contains(b)) {
            covered.extend(bonds);
            kept.push(ring.clone());
        }
    }
    if kept.is_empty() {
        rings
    } else {
        kept
    }
}

/// Practical SSSR for depiction: unique shortest cycles through each bond.
pub fn find_sssr(mol: &MoleculeIn, max_size: usize) -> Vec<Ring> {
    if mol.bonds.is_empty() {
        return Vec::new();
    }
    let adj = adjacency(mol);
    find_sssr_from_adj(&adj, mol.bonds.iter().map(|b| (b.begin, b.end)), max_size)
}

fn ring_centroid(ring: &Ring, coords: &HashMap<i32, (f64, f64)>) -> Option<(f64, f64)> {
    if ring.atoms.is_empty() {
        return None;
    }
    let mut sx = 0.0;
    let mut sy = 0.0;
    for &i in &ring.atoms {
        let (x, y) = *coords.get(&i)?;
        sx += x;
        sy += y;
    }
    let n = ring.atoms.len() as f64;
    Some((sx / n, sy / n))
}

/// Unit normals for ring bonds pointing toward the owning (smallest) ring centroid.
pub fn bond_interior_normals(
    rings: &[Ring],
    coords: &HashMap<i32, (f64, f64)>,
) -> HashMap<(i32, i32), (f64, f64)> {
    let mut normals = HashMap::new();
    let mut sorted: Vec<&Ring> = rings.iter().collect();
    sorted.sort_by_key(|r| r.size());
    for ring in sorted {
        let Some((cx, cy)) = ring_centroid(ring, coords) else {
            continue;
        };
        let n = ring.size();
        for i in 0..n {
            let a = ring.atoms[i];
            let b = ring.atoms[(i + 1) % n];
            let key = bond_key(a, b);
            if normals.contains_key(&key) {
                continue;
            }
            let Some(&(x1, y1)) = coords.get(&a) else {
                continue;
            };
            let Some(&(x2, y2)) = coords.get(&b) else {
                continue;
            };
            let dx = x2 - x1;
            let dy = y2 - y1;
            let length = (dx * dx + dy * dy).sqrt().max(1e-9);
            let mut nx = -dy / length;
            let mut ny = dx / length;
            let mx = 0.5 * (x1 + x2);
            let my = 0.5 * (y1 + y2);
            if (cx - mx) * nx + (cy - my) * ny < 0.0 {
                nx = -nx;
                ny = -ny;
            }
            normals.insert(key, (nx, ny));
        }
    }
    normals
}

fn ring_is_fully_aromatic(ring: &Ring, order_by_bond: &HashMap<(i32, i32), f64>) -> bool {
    let bonds = ring.bonds();
    !bonds.is_empty()
        && bonds
            .iter()
            .all(|k| order_by_bond.get(k).copied().is_some_and(is_aromatic_order))
}

/// Perpendicular distance from point to the infinite line through `a`–`b`.
fn point_line_distance(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    ((px - ax) * dy - (py - ay) * dx).abs() / len
}

/// Centroids + radii for SSSR rings whose every bond is aromatic-encoded.
///
/// Radius is [`AROMATIC_CIRCLE_FRAC`] × ring inradius (min centroid→edge
/// distance). Mixed Kekulé/aromatic rings are skipped.
pub fn aromatic_ring_circles(
    mol: &MoleculeIn,
    rings: &[Ring],
    coords: &HashMap<i32, (f64, f64)>,
) -> Vec<(f64, f64, f64)> {
    let mut order_by_bond: HashMap<(i32, i32), f64> = HashMap::with_capacity(mol.bonds.len());
    for b in &mol.bonds {
        order_by_bond.insert(bond_key(b.begin, b.end), b.order);
    }
    let mut out = Vec::new();
    for ring in rings {
        if !ring_is_fully_aromatic(ring, &order_by_bond) {
            continue;
        }
        let Some((cx, cy)) = ring_centroid(ring, coords) else {
            continue;
        };
        let n = ring.size();
        let mut inradius = f64::INFINITY;
        for i in 0..n {
            let a = ring.atoms[i];
            let b = ring.atoms[(i + 1) % n];
            let Some(&(x1, y1)) = coords.get(&a) else {
                continue;
            };
            let Some(&(x2, y2)) = coords.get(&b) else {
                continue;
            };
            inradius = inradius.min(point_line_distance(cx, cy, x1, y1, x2, y2));
        }
        if !inradius.is_finite() || inradius <= 1e-6 {
            continue;
        }
        let r = inradius * AROMATIC_CIRCLE_FRAC;
        if r > 1e-6 {
            out.push((cx, cy, r));
        }
    }
    out
}

/// Paint-only bond orders: assign alternating 2/1 around fully aromatic rings.
///
/// Does not mutate [`MoleculeIn`]. Shared bonds keep the first assignment
/// (smaller rings first).
pub fn kekule_paint_orders(mol: &MoleculeIn, rings: &[Ring]) -> HashMap<(i32, i32), f64> {
    let mut orders: HashMap<(i32, i32), f64> = HashMap::with_capacity(mol.bonds.len());
    for b in &mol.bonds {
        orders.insert(bond_key(b.begin, b.end), b.order);
    }
    let aromatic_rings: Vec<&Ring> = {
        let mut v: Vec<&Ring> = rings
            .iter()
            .filter(|r| ring_is_fully_aromatic(r, &orders))
            .collect();
        v.sort_by_key(|r| r.size());
        v
    };
    for ring in aromatic_rings {
        let n = ring.size();
        let mut start = 0usize;
        let mut want_double = true;
        for i in 0..n {
            let key = bond_key(ring.atoms[i], ring.atoms[(i + 1) % n]);
            let cur = orders.get(&key).copied().unwrap_or(1.0);
            if !is_aromatic_order(cur) {
                start = (i + 1) % n;
                want_double = !(cur >= 1.5 && cur < 2.5);
                break;
            }
        }
        for k in 0..n {
            let i = (start + k) % n;
            let key = bond_key(ring.atoms[i], ring.atoms[(i + 1) % n]);
            let cur = orders.get(&key).copied().unwrap_or(1.0);
            if is_aromatic_order(cur) {
                orders.insert(key, if want_double { 2.0 } else { 1.0 });
            } else {
                want_double = !(cur >= 1.5 && cur < 2.5);
                continue;
            }
            want_double = !want_double;
        }
    }
    orders
}

/// Fill [`BondIn::interior`] for ring bonds from SSSR normals.
///
/// Ring membership **overrules** the acyclic centered/extend path: any bond
/// that sits in a small ring gets an interior normal (caller value replaced).
pub fn apply_ring_interiors(mol: &mut MoleculeIn) {
    let rings = find_sssr(mol, 8);
    if rings.is_empty() {
        return;
    }
    let coords: HashMap<i32, (f64, f64)> = mol
        .atoms
        .iter()
        .map(|a: &AtomIn| (a.index, (a.x, a.y)))
        .collect();
    let normals = bond_interior_normals(&rings, &coords);
    for bond in &mut mol.bonds {
        let key = bond_key(bond.begin, bond.end);
        if let Some(n) = normals.get(&key) {
            bond.interior = Some(*n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::BondIn as SceneBond;

    fn benzene() -> MoleculeIn {
        // Regular hexagon, bond length 20.
        let mut atoms = Vec::new();
        for i in 0..6 {
            let ang = std::f64::consts::PI / 2.0 + i as f64 * std::f64::consts::TAU / 6.0;
            atoms.push(AtomIn {
                index: i,
                element: Some("C".into()),
                z: None,
                x: 20.0 * ang.cos(),
                y: 20.0 * ang.sin(),
                label: None,
                charge: 0,
                mapno: None,
            });
        }
        let orders = [1.0, 2.0, 1.0, 2.0, 1.0, 2.0];
        let mut bonds = Vec::new();
        for i in 0..6 {
            bonds.push(SceneBond {
                index: i,
                begin: i,
                end: (i + 1) % 6,
                order: orders[i as usize],
                stereo: None,
                interior: None,
            });
        }
        MoleculeIn {
            id: Some("benzene".into()),
            atoms,
            bonds,
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        }
    }

    #[test]
    fn benzene_sssr_is_one_hexagon() {
        let rings = find_sssr(&benzene(), 8);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].size(), 6);
    }

    #[test]
    fn aromatic_orders_yield_one_circle_kekule_yields_none() {
        let coords: HashMap<i32, (f64, f64)> = benzene()
            .atoms
            .iter()
            .map(|a| (a.index, (a.x, a.y)))
            .collect();
        let rings = find_sssr(&benzene(), 8);
        assert!(aromatic_ring_circles(&benzene(), &rings, &coords).is_empty());

        let mut aromatic = benzene();
        for b in &mut aromatic.bonds {
            b.order = 4.0;
        }
        let circles = aromatic_ring_circles(&aromatic, &rings, &coords);
        assert_eq!(circles.len(), 1);
        let (cx, cy, r) = circles[0];
        // AROMATIC_CIRCLE_FRAC = 0.60 → ~0.52 × bond for regular hex.
        assert!(r > 9.0 && r < 12.0, "near-edge circle, r={r}");
        assert!(cx.abs() < 1e-6 && cy.abs() < 1e-6);
    }

    #[test]
    fn benzene_doubles_get_interior() {
        let mut mol = benzene();
        apply_ring_interiors(&mut mol);
        let doubles: Vec<_> = mol.bonds.iter().filter(|b| b.order >= 1.5).collect();
        assert_eq!(doubles.len(), 3);
        for b in doubles {
            assert!(b.interior.is_some(), "bond {} missing interior", b.index);
        }
    }

    fn ethanol_chain() -> MoleculeIn {
        MoleculeIn {
            id: Some("etoh".into()),
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: Some("C".into()),
                    z: None,
                    x: 0.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: None,
                    x: 20.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                    mapno: None,
                },
                AtomIn {
                    index: 2,
                    element: Some("O".into()),
                    z: None,
                    x: 30.0,
                    y: 10.0,
                    label: Some("OH".into()),
                    charge: 0,
                    mapno: None,
                },
            ],
            bonds: vec![
                SceneBond {
                    index: 0,
                    begin: 0,
                    end: 1,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
                SceneBond {
                    index: 1,
                    begin: 1,
                    end: 2,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
            ],
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        }
    }

    #[test]
    fn acyclic_find_sssr_empty_and_apply_is_noop() {
        let mut mol = ethanol_chain();
        assert!(find_sssr(&mol, 8).is_empty());
        apply_ring_interiors(&mut mol);
        assert!(mol.bonds.iter().all(|b| b.interior.is_none()));

        let empty = MoleculeIn {
            id: None,
            atoms: vec![],
            bonds: vec![],
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        };
        assert!(find_sssr(&empty, 8).is_empty());
    }

    #[test]
    fn naphthalene_sssr_has_two_hexagons() {
        // Two fused hexagons sharing edge 0–1; atoms 6..=9 are reflections of 2..=5.
        let mut atoms = Vec::new();
        for i in 0..6 {
            let ang = std::f64::consts::PI / 2.0 + i as f64 * std::f64::consts::TAU / 6.0;
            atoms.push(AtomIn {
                index: i,
                element: Some("C".into()),
                z: None,
                x: 20.0 * ang.cos(),
                y: 20.0 * ang.sin(),
                label: None,
                charge: 0,
                mapno: None,
            });
        }
        let mx = 0.5 * (atoms[0].x + atoms[1].x);
        let my = 0.5 * (atoms[0].y + atoms[1].y);
        let reflected: Vec<(f64, f64)> = (2..6)
            .map(|i| {
                let a = &atoms[i as usize];
                (mx - (a.x - mx), my - (a.y - my))
            })
            .collect();
        for (k, &(x, y)) in reflected.iter().enumerate() {
            atoms.push(AtomIn {
                index: 6 + k as i32,
                element: Some("C".into()),
                z: None,
                x,
                y,
                label: None,
                charge: 0,
                mapno: None,
            });
        }
        let mut bonds = Vec::new();
        let mut bi = 0i32;
        for i in 0..6 {
            bonds.push(SceneBond {
                index: bi,
                begin: i,
                end: (i + 1) % 6,
                order: 1.0,
                stereo: None,
                interior: None,
            });
            bi += 1;
        }
        for &(a, b) in &[(1, 6), (6, 7), (7, 8), (8, 9), (9, 0)] {
            bonds.push(SceneBond {
                index: bi,
                begin: a,
                end: b,
                order: 1.0,
                stereo: None,
                interior: None,
            });
            bi += 1;
        }
        let mol = MoleculeIn {
            id: Some("naph".into()),
            atoms,
            bonds,
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            weight: 1.0,
            scale: 1.0,
            aromatic: Default::default(),
            debug_box: false,
            debug_align: false,
            align_debug: None,
        };
        let rings = find_sssr(&mol, 8);
        assert_eq!(rings.len(), 2, "expected two hexagons, got {:?}", rings);
        assert!(rings.iter().all(|r| r.size() == 6));
    }

    #[test]
    fn large_cycle_sssr_needs_max_size() {
        let n = 12i32;
        let mut adj: HashMap<i32, Vec<i32>> = HashMap::new();
        let mut bonds = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            adj.entry(i).or_default().push(j);
            adj.entry(j).or_default().push(i);
            bonds.push((i, j));
        }
        assert!(find_sssr_from_adj(&adj, bonds.iter().copied(), 8).is_empty());
        let rings = find_sssr_from_adj(&adj, bonds, 20);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].size(), 12);
    }

    fn cube_graph() -> (HashMap<i32, Vec<i32>>, Vec<(i32, i32)>) {
        // Cube: cyclomatic number 5, six square faces.
        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        let mut adj: HashMap<i32, Vec<i32>> = HashMap::new();
        for (a, b) in edges {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        (adj, edges.to_vec())
    }

    #[test]
    fn cubane_relevant_rings_keep_all_six_faces() {
        let (adj, bonds) = cube_graph();
        let sssr = find_sssr_from_adj(&adj, bonds.iter().copied(), 8);
        assert_eq!(sssr.len(), 5, "SSSR is a 5-cycle basis, got {:?}", sssr);
        assert!(sssr.iter().all(|r| r.size() == 4));

        let relevant = find_relevant_rings_from_adj(&adj, bonds, 8);
        assert_eq!(
            relevant.len(),
            6,
            "relevant rings are all six faces, got {:?}",
            relevant
        );
        assert!(relevant.iter().all(|r| r.size() == 4));
    }

    #[test]
    fn naphthalene_relevant_rings_match_two_hexagons() {
        // Two fused hexagons: 0-5 and 4-5-6-7-8-9.
        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (4, 5),
            (5, 0),
            (4, 6),
            (6, 7),
            (7, 8),
            (8, 9),
            (9, 5),
        ];
        let mut adj: HashMap<i32, Vec<i32>> = HashMap::new();
        for (a, b) in edges {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        let relevant = find_relevant_rings_from_adj(&adj, edges, 8);
        assert_eq!(relevant.len(), 2, "got {:?}", relevant);
        assert!(relevant.iter().all(|r| r.size() == 6));
    }
}

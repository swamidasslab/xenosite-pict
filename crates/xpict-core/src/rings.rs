//! Practical SSSR and ring-interior normals for depiction.
//!
//! Layout backends own coordinates. This module only supplies drawing helpers:
//! which bonds sit in small rings, and unit normals toward the ring centroid
//! so Kekulé offsets stay short and inside (not the acyclic extend-onto-singles
//! path). Mirrors `src/xpict/draw/rings.py`.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::scene::{AtomIn, MoleculeIn};

/// One SSSR ring as an ordered atom cycle (atom indices).
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
    let mut adj: HashMap<i32, Vec<i32>> = mol
        .atoms
        .iter()
        .map(|a| (a.index, Vec::new()))
        .collect();
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

/// Practical SSSR for depiction: unique shortest cycles through each bond.
pub fn find_sssr(mol: &MoleculeIn, max_size: usize) -> Vec<Ring> {
    if mol.bonds.is_empty() {
        return Vec::new();
    }
    let adj = adjacency(mol);
    let mut by_bonds: HashMap<Vec<(i32, i32)>, Ring> = HashMap::new();
    for b in &mol.bonds {
        let Some(cyc) = shortest_cycle_through_bond(&adj, b.begin, b.end, max_size) else {
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
    rings.sort_by(|a, b| {
        a.size()
            .cmp(&b.size())
            .then_with(|| a.atoms.cmp(&b.atoms))
    });
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
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        }
    }

    #[test]
    fn benzene_sssr_is_one_hexagon() {
        let rings = find_sssr(&benzene(), 8);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].size(), 6);
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
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: None,
                    x: 20.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
                AtomIn {
                    index: 2,
                    element: Some("O".into()),
                    z: None,
                    x: 30.0,
                    y: 10.0,
                    label: Some("OH".into()),
                    charge: 0,
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
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
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
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
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
            mark_atoms: vec![],
            mark_bonds: vec![],
            bold_labels: false,
        };
        let rings = find_sssr(&mol, 8);
        assert_eq!(rings.len(), 2, "expected two hexagons, got {:?}", rings);
        assert!(rings.iter().all(|r| r.size() == 6));
    }
}

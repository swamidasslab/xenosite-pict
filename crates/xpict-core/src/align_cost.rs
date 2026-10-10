//! Minimum-cost mapping on the atom–bond incidence graph.
//!
//! Atoms and bonds are both vertices; edges only join an atom to its incident
//! bonds. Match atom↔atom and bond↔bond with a simple additive cost. The only
//! hard rule is incompatible E/Z substituent side pairings. No phantom bonds.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EzStereo {
    None,
    Cis,
    Trans,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomChirality {
    None,
    R,
    S,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomFeat {
    pub atomic_number: u8,
    pub formal_charge: i8,
    pub radical: u8,
    pub chirality: AtomChirality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BondFeat {
    pub begin: u16,
    pub end: u16,
    pub order: u8,
    pub aromatic: bool,
    pub stereo: EzStereo,
    pub stereo_atom1: Option<u16>,
    pub stereo_atom2: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct MolGraph {
    pub atoms: Vec<AtomFeat>,
    pub bonds: Vec<BondFeat>,
}

impl MolGraph {
    pub fn new(atoms: Vec<AtomFeat>, bonds: Vec<BondFeat>) -> Self {
        Self { atoms, bonds }
    }

    pub fn n_atoms(&self) -> usize {
        self.atoms.len()
    }

    pub fn n_bonds(&self) -> usize {
        self.bonds.len()
    }

    /// Relabel atoms by `perm` where `perm[old] = new`.
    pub fn permute(&self, perm: &[u16]) -> Self {
        assert_eq!(perm.len(), self.n_atoms());
        let n = self.n_atoms();
        let mut atoms = vec![
            AtomFeat {
                atomic_number: 0,
                formal_charge: 0,
                radical: 0,
                chirality: AtomChirality::None,
            };
            n
        ];
        for (old, &new) in perm.iter().enumerate() {
            atoms[new as usize] = self.atoms[old];
        }
        let map_opt = |o: Option<u16>| o.map(|i| perm[i as usize]);
        let bonds = self
            .bonds
            .iter()
            .map(|b| BondFeat {
                begin: perm[b.begin as usize],
                end: perm[b.end as usize],
                order: b.order,
                aromatic: b.aromatic,
                stereo: b.stereo,
                stereo_atom1: map_opt(b.stereo_atom1),
                stereo_atom2: map_opt(b.stereo_atom2),
            })
            .collect();
        Self { atoms, bonds }
    }
}

pub const COST_ELEMENT: i32 = 3;
pub const COST_CHARGE_RADICAL: i32 = 2;
pub const COST_BOND_ORDER: i32 = 3;
/// Unmatched atom/bond vertices — strictly above any pair-mismatch cost.
pub const COST_UNMATCHED: i32 = 5;

/// Incidence graph: vertices `[0..n_atoms)` atoms, `[n_atoms..n_atoms+n_bonds)` bonds.
#[derive(Debug, Clone)]
struct AbGraph {
    mol: MolGraph,
    adj: Vec<Vec<u16>>,
}

impl AbGraph {
    fn from_mol(mol: MolGraph) -> Self {
        let n = mol.n_atoms();
        let m = mol.n_bonds();
        let mut adj = vec![Vec::new(); n + m];
        for (bi, b) in mol.bonds.iter().enumerate() {
            let bv = (n + bi) as u16;
            let a = b.begin;
            let c = b.end;
            adj[a as usize].push(bv);
            adj[c as usize].push(bv);
            adj[bv as usize].push(a);
            adj[bv as usize].push(c);
        }
        Self { mol, adj }
    }

    fn n_atoms(&self) -> usize {
        self.mol.n_atoms()
    }

    fn n_verts(&self) -> usize {
        self.mol.n_atoms() + self.mol.n_bonds()
    }

    fn is_atom(&self, v: u16) -> bool {
        (v as usize) < self.n_atoms()
    }

    fn bond_index(&self, v: u16) -> Option<usize> {
        let n = self.n_atoms();
        let i = v as usize;
        if i >= n {
            Some(i - n)
        } else {
            None
        }
    }
}

fn atom_cost(a: &AtomFeat, b: &AtomFeat) -> i32 {
    let mut c = 0;
    if a.atomic_number != b.atomic_number {
        c += COST_ELEMENT;
    }
    if a.formal_charge != b.formal_charge || a.radical != b.radical {
        c += COST_CHARGE_RADICAL;
    }
    c
}

fn bond_cost(a: &BondFeat, b: &BondFeat) -> i32 {
    if a.aromatic && b.aromatic {
        return 0;
    }
    if a.order == b.order {
        return 0;
    }
    COST_BOND_ORDER
}

fn pair_cost(q: &AbGraph, r: &AbGraph, qv: u16, rv: u16) -> i32 {
    match (q.bond_index(qv), r.bond_index(rv)) {
        (None, None) => atom_cost(
            &q.mol.atoms[qv as usize],
            &r.mol.atoms[rv as usize],
        ),
        (Some(qi), Some(ri)) => bond_cost(&q.mol.bonds[qi], &r.mol.bonds[ri]),
        _ => i32::MAX / 4, // type mismatch — should not be proposed
    }
}

/// Required query stereo-atom → ref stereo-atom when both ends of a stereo
/// double bond are mapped. `u16::MAX` = must not land on any alkene substituent
/// slot of the ref bond (cis vs trans).
fn ez_required(q: &MolGraph, r: &MolGraph, atom_map: &HashMap<u16, u16>) -> HashMap<u16, u16> {
    let mut req = HashMap::new();
    for qb in &q.bonds {
        if qb.stereo == EzStereo::None {
            continue;
        }
        let (Some(&ra), Some(&rb)) = (atom_map.get(&qb.begin), atom_map.get(&qb.end)) else {
            continue;
        };
        let Some(rbond) = r.bonds.iter().find(|b| {
            (b.begin == ra && b.end == rb) || (b.begin == rb && b.end == ra)
        }) else {
            continue;
        };
        if rbond.stereo == EzStereo::None {
            continue;
        }
        let (Some(q1), Some(q2), Some(r1), Some(r2)) = (
            qb.stereo_atom1,
            qb.stereo_atom2,
            rbond.stereo_atom1,
            rbond.stereo_atom2,
        ) else {
            continue;
        };
        let same = atom_map.get(&qb.begin) == Some(&rbond.begin)
            && atom_map.get(&qb.end) == Some(&rbond.end);
        let flip = atom_map.get(&qb.begin) == Some(&rbond.end)
            && atom_map.get(&qb.end) == Some(&rbond.begin);
        if !same && !flip {
            continue;
        }
        if qb.stereo != rbond.stereo {
            req.insert(q1, u16::MAX);
            req.insert(q2, u16::MAX);
        } else if flip {
            req.insert(q1, r2);
            req.insert(q2, r1);
        } else {
            req.insert(q1, r1);
            req.insert(q2, r2);
        }
    }
    req
}

fn alkene_slot(mol: &MolGraph, bond: &BondFeat, atom: u16) -> bool {
    if atom == bond.begin || atom == bond.end {
        return false;
    }
    mol.bonds.iter().any(|b| {
        (b.begin == atom && (b.end == bond.begin || b.end == bond.end))
            || (b.end == atom && (b.begin == bond.begin || b.begin == bond.end))
    })
}

fn ez_ok(q: &MolGraph, r: &MolGraph, atom_map: &HashMap<u16, u16>, on: u16, rn: u16) -> bool {
    let mut trial = atom_map.clone();
    trial.insert(on, rn);
    let req = ez_required(q, r, &trial);
    for (qa, &want) in &req {
        let Some(&got) = trial.get(qa) else {
            continue;
        };
        if want == u16::MAX {
            // Cis≠trans: no landing on any substituent of the matched stereo bond.
            let blocked = q.bonds.iter().any(|qb| {
                if qb.stereo == EzStereo::None
                    || (qb.stereo_atom1 != Some(*qa) && qb.stereo_atom2 != Some(*qa))
                {
                    return false;
                }
                let (Some(&ra), Some(&rb)) = (trial.get(&qb.begin), trial.get(&qb.end)) else {
                    return false;
                };
                r.bonds.iter().any(|rbond| {
                    rbond.stereo != EzStereo::None
                        && ((rbond.begin == ra && rbond.end == rb)
                            || (rbond.begin == rb && rbond.end == ra))
                        && alkene_slot(r, rbond, got)
                })
            });
            if blocked {
                return false;
            }
        } else if got != want {
            // Wrong side of a compatible cis/cis or trans/trans pair.
            let q_stereo = q.bonds.iter().any(|b| {
                b.stereo != EzStereo::None
                    && (b.stereo_atom1 == Some(*qa) || b.stereo_atom2 == Some(*qa))
            });
            let r_stereo = r.bonds.iter().any(|b| {
                b.stereo != EzStereo::None
                    && (b.stereo_atom1 == Some(got) || b.stereo_atom2 == Some(got))
            });
            if q_stereo && r_stereo {
                return false;
            }
        }
    }
    true
}

fn atom_map_of(q: &AbGraph, mapping: &HashMap<u16, u16>) -> HashMap<u16, u16> {
    mapping
        .iter()
        .filter(|(&qv, _)| q.is_atom(qv))
        .map(|(&qv, &rv)| (qv, rv))
        .collect()
}

fn compatible(q: &AbGraph, r: &AbGraph, mapping: &HashMap<u16, u16>, qv: u16, rv: u16) -> bool {
    if q.is_atom(qv) != r.is_atom(rv) {
        return false;
    }
    // Incidence consistency with already-mapped neighbors.
    for &qn in &q.adj[qv as usize] {
        let Some(&rn) = mapping.get(&qn) else {
            continue;
        };
        if !r.adj[rv as usize].contains(&rn) {
            return false;
        }
    }
    if q.is_atom(qv) {
        let am = atom_map_of(q, mapping);
        if !ez_ok(&q.mol, &r.mol, &am, qv, rv) {
            return false;
        }
    }
    true
}

fn mapping_cost_ab(q: &AbGraph, r: &AbGraph, mapping: &HashMap<u16, u16>) -> i32 {
    let mut cost = 0i32;
    for (&qv, &rv) in mapping {
        cost = cost.saturating_add(pair_cost(q, r, qv, rv));
    }
    for qv in 0..q.n_verts() as u16 {
        if !mapping.contains_key(&qv) {
            cost += COST_UNMATCHED;
        }
    }
    cost
}

const NODE_BUDGET: usize = 12_000;
const MAP_KEEP: usize = 32;
const SEED_CAP: usize = 48;

fn grow_from_seed(
    q: &AbGraph,
    r: &AbGraph,
    seed_q: u16,
    seed_r: u16,
    budget: usize,
) -> (Vec<HashMap<u16, u16>>, usize) {
    let mut found = Vec::new();
    let mut best_cost = i32::MAX;
    let mut nodes = 0usize;

    fn rec(
        q: &AbGraph,
        r: &AbGraph,
        mapping: &mut HashMap<u16, u16>,
        inv: &mut HashMap<u16, u16>,
        found: &mut Vec<HashMap<u16, u16>>,
        best_cost: &mut i32,
        nodes: &mut usize,
        budget: usize,
    ) {
        if *nodes >= budget {
            return;
        }
        *nodes += 1;

        let mut growable: Vec<(usize, u16, Vec<u16>)> = Vec::new();
        for (&qv, &rv) in mapping.iter() {
            for &qn in &q.adj[qv as usize] {
                if mapping.contains_key(&qn) {
                    continue;
                }
                let mut cands = Vec::new();
                for &rn in &r.adj[rv as usize] {
                    if inv.contains_key(&rn) {
                        continue;
                    }
                    if compatible(q, r, mapping, qn, rn) {
                        cands.push(rn);
                    }
                }
                if !cands.is_empty() {
                    growable.push((cands.len(), qn, cands));
                }
            }
        }

        if growable.is_empty() {
            let cost = mapping_cost_ab(q, r, mapping);
            if cost < *best_cost {
                *best_cost = cost;
                found.clear();
                found.push(mapping.clone());
            } else if cost == *best_cost && found.len() < MAP_KEEP {
                found.push(mapping.clone());
            }
            return;
        }

        growable.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let (_n, qn, mut cands) = growable.into_iter().next().unwrap();
        cands.sort_unstable();
        for rn in cands {
            mapping.insert(qn, rn);
            inv.insert(rn, qn);
            rec(q, r, mapping, inv, found, best_cost, nodes, budget);
            mapping.remove(&qn);
            inv.remove(&rn);
            if *nodes >= budget {
                return;
            }
        }
    }

    let mut mapping = HashMap::from([(seed_q, seed_r)]);
    let mut inv = HashMap::from([(seed_r, seed_q)]);
    rec(
        q,
        r,
        &mut mapping,
        &mut inv,
        &mut found,
        &mut best_cost,
        &mut nodes,
        budget,
    );
    (found, nodes)
}

#[derive(Debug, Clone)]
pub struct MappingResult {
    /// query atom idx → ref atom idx
    pub map: HashMap<u16, u16>,
    pub cost: i32,
}

/// Minimum-cost connected mapping of `query` onto `reference`.
pub fn cost_mapping(reference: &MolGraph, query: &MolGraph) -> Option<MappingResult> {
    if reference.n_atoms() == 0 || query.n_atoms() == 0 {
        return None;
    }
    let r = AbGraph::from_mol(reference.clone());
    let q = AbGraph::from_mol(query.clone());

    // Seed on atom pairs (prefer exact element, then hetero, then degree).
    let mut seeds = Vec::new();
    for qa in 0..q.n_atoms() as u16 {
        for ra in 0..r.n_atoms() as u16 {
            seeds.push((qa, ra));
        }
    }
    seeds.sort_by(|a, b| {
        let aq = &q.mol.atoms[a.0 as usize];
        let ar = &r.mol.atoms[a.1 as usize];
        let bq = &q.mol.atoms[b.0 as usize];
        let br = &r.mol.atoms[b.1 as usize];
        let a_ex = aq.atomic_number == ar.atomic_number;
        let b_ex = bq.atomic_number == br.atomic_number;
        b_ex.cmp(&a_ex)
            .then((aq.atomic_number != 6).cmp(&(bq.atomic_number != 6)))
            .then(q.adj[b.0 as usize].len().cmp(&q.adj[a.0 as usize].len()))
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });

    let mut budget = NODE_BUDGET;
    let mut best: Option<MappingResult> = None;
    for (sq, sr) in seeds.into_iter().take(SEED_CAP.max(q.n_atoms() * 2)) {
        if budget == 0 {
            break;
        }
        let (maps, used) = grow_from_seed(&q, &r, sq, sr, budget);
        budget = budget.saturating_sub(used);
        for m in maps {
            let cost = mapping_cost_ab(&q, &r, &m);
            let atom_map = atom_map_of(&q, &m);
            let cand = MappingResult {
                map: atom_map,
                cost,
            };
            if best.as_ref().is_none_or(|b| cand.cost < b.cost) {
                best = Some(cand);
            }
        }
    }
    best
}

/// Cost of an explicit atom map (bonds inferred where both endpoints mapped).
pub fn mapping_cost(query: &MolGraph, reference: &MolGraph, mapping: &HashMap<u16, u16>) -> i32 {
    let q = AbGraph::from_mol(query.clone());
    let r = AbGraph::from_mol(reference.clone());
    let mut full = mapping.clone();
    // Add bond vertices whose endpoints are mapped to a ref bond's endpoints.
    for (qi, qb) in query.bonds.iter().enumerate() {
        let (Some(&ra), Some(&rb)) = (mapping.get(&qb.begin), mapping.get(&qb.end)) else {
            continue;
        };
        if let Some((ri, _)) = reference.bonds.iter().enumerate().find(|(_, b)| {
            (b.begin == ra && b.end == rb) || (b.begin == rb && b.end == ra)
        }) {
            full.insert((q.n_atoms() + qi) as u16, (r.n_atoms() + ri) as u16);
        }
    }
    mapping_cost_ab(&q, &r, &full)
}

pub fn signed_side(ax: f64, ay: f64, bx: f64, by: f64, px: f64, py: f64) -> f64 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

pub fn substituents_same_side(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    s1x: f64,
    s1y: f64,
    s2x: f64,
    s2y: f64,
) -> bool {
    let s1 = signed_side(ax, ay, bx, by, s1x, s1y);
    let s2 = signed_side(ax, ay, bx, by, s2x, s2y);
    s1 * s2 > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c() -> AtomFeat {
        AtomFeat {
            atomic_number: 6,
            formal_charge: 0,
            radical: 0,
            chirality: AtomChirality::None,
        }
    }

    fn single(a: u16, b: u16) -> BondFeat {
        BondFeat {
            begin: a,
            end: b,
            order: 1,
            aromatic: false,
            stereo: EzStereo::None,
            stereo_atom1: None,
            stereo_atom2: None,
        }
    }

    fn chain(n: usize) -> MolGraph {
        MolGraph::new(
            vec![c(); n],
            (0..n - 1).map(|i| single(i as u16, (i + 1) as u16)).collect(),
        )
    }

    fn cyclohexane() -> MolGraph {
        let mut bonds: Vec<_> = (0..5).map(|i| single(i, i + 1)).collect();
        bonds.push(single(0, 5));
        MolGraph::new(vec![c(); 6], bonds)
    }

    fn alkene(cis: bool) -> MolGraph {
        MolGraph::new(
            vec![c(); 4],
            vec![
                single(0, 1),
                BondFeat {
                    begin: 1,
                    end: 2,
                    order: 2,
                    aromatic: false,
                    stereo: if cis { EzStereo::Cis } else { EzStereo::Trans },
                    stereo_atom1: Some(0),
                    stereo_atom2: Some(3),
                },
                single(2, 3),
            ],
        )
    }

    fn hetero_alkene(cis: bool) -> MolGraph {
        let mut atoms = vec![c(); 4];
        atoms[0].atomic_number = 17;
        atoms[3].atomic_number = 35;
        MolGraph::new(
            atoms,
            vec![
                single(0, 1),
                BondFeat {
                    begin: 1,
                    end: 2,
                    order: 2,
                    aromatic: false,
                    stereo: if cis { EzStereo::Cis } else { EzStereo::Trans },
                    stereo_atom1: Some(0),
                    stereo_atom2: Some(3),
                },
                single(2, 3),
            ],
        )
    }

    fn branched_alkene(cis: bool) -> MolGraph {
        MolGraph::new(
            vec![c(); 6],
            vec![
                single(0, 1),
                BondFeat {
                    begin: 1,
                    end: 2,
                    order: 2,
                    aromatic: false,
                    stereo: if cis { EzStereo::Cis } else { EzStereo::Trans },
                    stereo_atom1: Some(0),
                    stereo_atom2: Some(3),
                },
                single(2, 3),
                single(1, 4),
                single(2, 5),
            ],
        )
    }

    fn chiral(r: bool) -> MolGraph {
        let mut atoms = vec![c(); 4];
        atoms[2].atomic_number = 8;
        atoms[3].atomic_number = 17;
        atoms[1].chirality = if r {
            AtomChirality::R
        } else {
            AtomChirality::S
        };
        MolGraph::new(atoms, vec![single(0, 1), single(1, 2), single(1, 3)])
    }

    fn all_perms(n: usize) -> Vec<Vec<u16>> {
        let mut base: Vec<u16> = (0..n as u16).collect();
        let mut out = Vec::new();
        fn heap(a: &mut [u16], size: usize, out: &mut Vec<Vec<u16>>) {
            if size == 1 {
                out.push(a.to_vec());
                return;
            }
            for i in 0..size {
                heap(a, size - 1, out);
                if size % 2 == 0 {
                    a.swap(i, size - 1);
                } else {
                    a.swap(0, size - 1);
                }
            }
        }
        heap(&mut base, n, &mut out);
        out
    }

    #[test]
    fn hexane_maps_all_cyclohexane_atoms() {
        let res = cost_mapping(&chain(6), &cyclohexane()).expect("map");
        assert_eq!(res.map.len(), 6, "map={:?}", res.map);
        // One unmatched ring-closure bond (cost > any pair mismatch).
        assert_eq!(res.cost, COST_UNMATCHED, "cost={}", res.cost);
    }

    #[test]
    fn element_mismatch_allowed() {
        let mut a = chain(2);
        a.atoms[1].atomic_number = 7;
        let res = cost_mapping(&chain(2), &a).expect("map");
        assert_eq!(res.map.len(), 2);
        assert!(res.cost >= COST_ELEMENT);
    }

    #[test]
    fn ez_self_all_orderings_zero_cost() {
        for g in [alkene(true), alkene(false), hetero_alkene(true), branched_alkene(true)] {
            for perm in all_perms(g.n_atoms()) {
                let gp = g.permute(&perm);
                let res = cost_mapping(&g, &gp).expect("map");
                assert_eq!(res.cost, 0, "perm={perm:?} map={:?}", res.map);
                assert_eq!(res.map.len(), g.n_atoms());
            }
        }
    }

    #[test]
    fn ez_cis_vs_trans_never_zero() {
        // Identity orderings + a sample of perms (full 6!² is too heavy).
        let pairs = [
            (alkene(true), alkene(false)),
            (hetero_alkene(true), hetero_alkene(false)),
            (branched_alkene(true), branched_alkene(false)),
        ];
        for (cis, trans) in pairs {
            let res = cost_mapping(&cis, &trans).expect("map");
            assert!(res.cost > 0, "identity cost=0 map={:?}", res.map);
            for perm in all_perms(cis.n_atoms()).into_iter().take(24) {
                let c = cis.permute(&perm);
                let res = cost_mapping(&c, &trans).expect("map");
                assert!(res.cost > 0, "perm={perm:?} cost=0 map={:?}", res.map);
            }
        }
    }

    #[test]
    fn ez_geometry_sides_and_distances() {
        let cis = [(-1.0, 1.0), (0.0, 0.0), (1.5, 0.0), (2.5, 1.0)];
        let trans = [(-1.0, 1.0), (0.0, 0.0), (1.5, 0.0), (2.5, -1.0)];
        assert!(substituents_same_side(
            cis[1].0, cis[1].1, cis[2].0, cis[2].1, cis[0].0, cis[0].1, cis[3].0, cis[3].1
        ));
        assert!(!substituents_same_side(
            trans[1].0, trans[1].1, trans[2].0, trans[2].1, trans[0].0, trans[0].1, trans[3].0,
            trans[3].1
        ));
        let d = |p: &[(f64, f64)], i: usize, j: usize| -> f64 {
            ((p[i].0 - p[j].0).powi(2) + (p[i].1 - p[j].1).powi(2)).sqrt()
        };
        assert!(d(&cis, 0, 3) < d(&trans, 0, 3));
    }

    #[test]
    fn tetrahedral_stereo_free() {
        let r = chiral(true);
        let s = chiral(false);
        let res = cost_mapping(&r, &s).expect("map");
        assert_eq!(res.cost, 0);
        assert_eq!(res.map.len(), 4);
        for perm in all_perms(4) {
            let res = cost_mapping(&r, &s.permute(&perm)).expect("map");
            assert_eq!(res.cost, 0, "perm={perm:?}");
        }
    }
}

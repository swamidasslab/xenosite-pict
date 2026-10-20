//! Exact connected molecular alignment with atom-only branch-and-bound.
//!
//! Equal-size Relevant Rings are tried first as connected seeds (cyclic maps,
//! unique ring sizes preferred) so fused scaffolds lock before the house
//! budget is spent. Chains still map onto rings: no query ring means the
//! usual atom-pair seeds run (hexane onto cyclohexane). Different-size
//! rings are not seeded as cycles; ordinary MCS may still map a path.
//!
//! [`cost_mapping`] / [`cost_mapping_with_generators`] stay unbounded (library
//! exact API). Depict / EdgePlan search uses [`SearchLimits`]: a house time and
//! node cap, optional greedy-only, and timeout returns the greedy/BnB incumbent.
//! Worst-case search remains exponential when uncapped.
//! Unmatched atoms and bonds are penalized on BOTH sides (query and reference).
//! Bond correspondence is inferred from atom endpoints; missing bonds are allowed.
//! Returns one optimum, not every symmetry-related map or a geometric tie-break.
//! This is a standalone replacement module using the original public graph types.
//! Minimum cost can omit an expensive terminal atom; two original tests requiring
//! a more costly extension have been corrected accordingly.
//!
//! Entry points:
//! ```ignore
//! let result = cost_mapping(&reference, &query); // exact, without symmetry input
//! let (result, stats) = cost_mapping_with_generators(
//!     &reference, &query, &reference_generators,
//! ).expect("valid graphs and automorphism generators");
//! let (result, stats) = cost_mapping_with_generators_limited(
//!     &reference, &query, &reference_generators, &SearchLimits::house(),
//! ).expect("valid graphs");
//! ```
//! Each generator is `Vec<u16>` with `generator[old_atom] = new_atom`.
//! Project Cannonaut incidence-vertex permutations onto atom vertices first.
//! An empty generator list is valid and only disables symmetry acceleration.
//! Generators need not generate the whole automorphism group: any valid subgroup
//! is safe. Generators are checked against element, charge, radical, and exact
//! bond order/aromaticity. Stereo-participating reference atoms are then fixed
//! with Schreier stabilizers, conservatively preserving the supplied E/Z policy.
//! No dependency on a particular Cannonaut crate API is assumed.
//!
//! E/Z follows the attachment's restrictive side-pairing policy, with every
//! constraint checked independently (no overwritten HashMap requirements).
//! It is not a new general chemical stereoisomorphism implementation.
//! Tetrahedral chirality is intentionally ignored, as in the attachment.
//!
//! [`cost_mapping`] feeds reference automorphism generators from Canonaut
//! (nauty port) on the colored atom–bond incidence graph.

use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use std::time::Instant;

use crate::rings::find_relevant_rings_from_adj;

/// House wall-clock budget for document / EdgePlan MCS (not the unbounded
/// [`cost_mapping`] library entry).
pub const ALIGN_HOUSE_TIMEOUT_MS: u64 = 100;
/// House branch-and-bound state cap (wasm-safe even if `Instant` is inert).
pub const ALIGN_HOUSE_MAX_STATES: u64 = 50_000;
/// Inclusive Relevant-Ring cap for align seeds (crowns / macrolides).
const ALIGN_RING_MAX: usize = 40;
/// Max cyclic ring maps tried before ordinary atom-pair seeds.
const MAX_RING_SEEDS: usize = 32;

/// Caps for [`cost_mapping_with_generators_limited`]. Empty limits = unbounded.
#[derive(Debug, Clone, Default)]
pub struct SearchLimits {
    /// Stop when `Instant::now()` reaches this (native). `None` = no clock cap.
    pub deadline: Option<Instant>,
    /// Stop when [`SearchStats::states`] reaches this. `None` = no node cap.
    pub max_states: Option<u64>,
    /// Greedy incumbent only — skip branch-and-bound `dfs`.
    pub greedy_only: bool,
}

impl SearchLimits {
    /// No time, node, or greedy-only caps.
    pub fn unbounded() -> Self {
        Self::default()
    }

    /// Depict-path default: [`ALIGN_HOUSE_TIMEOUT_MS`] + [`ALIGN_HOUSE_MAX_STATES`].
    pub fn house() -> Self {
        Self::timed(ALIGN_HOUSE_TIMEOUT_MS)
    }

    /// Wall-clock budget plus the house node cap. `ms == 0` is unbounded.
    pub fn timed(ms: u64) -> Self {
        if ms == 0 {
            return Self::unbounded();
        }
        Self {
            deadline: deadline_from_ms(ms),
            max_states: Some(ALIGN_HOUSE_MAX_STATES),
            greedy_only: false,
        }
    }

    /// Wire `timeout_ms`: `None` = house, `Some(0)` = unbounded, else that many ms.
    pub fn from_timeout_ms(timeout_ms: Option<u32>, greedy_only: bool) -> Self {
        let mut limits = match timeout_ms {
            Some(0) => Self::unbounded(),
            Some(ms) => Self::timed(u64::from(ms)),
            None => Self::house(),
        };
        limits.greedy_only = greedy_only;
        limits
    }
}

fn deadline_from_ms(ms: u64) -> Option<Instant> {
    // wasm32 Instant may be inert or panic; node cap is the hatch there.
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ms;
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Some(Instant::now() + Duration::from_millis(ms))
    }
}

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
pub const COST_ELEMENT: i32 = 11;
pub const COST_CHARGE_RADICAL: i32 = 2;
pub const COST_BOND_ORDER: i32 = 3;
pub const COST_UNMATCHED: i32 = 5;
pub const COST_RING: i32 = 1;
const NONE: usize = usize::MAX;
type AtomType = (u8, i8, u8, bool, usize, bool);

#[derive(Debug, Clone)]
pub struct MappingResult {
    pub map: HashMap<u16, u16>,
    pub cost: i32,
}

#[derive(Debug, Default, Clone)]
pub struct SearchStats {
    pub seeds: u64,
    pub states: u64,
    pub bound_prunes: u64,
    pub assignment_bounds: u64,
    pub symmetry_alternatives_removed: u64,
    /// True when a time or node cap stopped the search; incumbent is still valid.
    pub timed_out: bool,
}

struct Graph<'a> {
    mol: &'a MolGraph,
    n: usize,
    // Flat endpoint lookup; NONE denotes a missing edge.
    edge: Vec<usize>,
    adj: Vec<Vec<(usize, usize)>>,
    ring: Vec<bool>,
    stereo_bonds: Vec<usize>,
    stereo_designated: Vec<bool>,
}
impl<'a> Graph<'a> {
    fn new(mol: &'a MolGraph) -> Result<Self, String> {
        let n = mol.n_atoms();
        if n > u16::MAX as usize {
            return Err("atom count exceeds u16 indexing".into());
        }
        let mut edge = vec![NONE; n * n];
        let mut adj = vec![Vec::new(); n];
        let mut stereo_bonds = Vec::new();
        let mut stereo_designated = vec![false; n];
        for (i, b) in mol.bonds.iter().enumerate() {
            let (a, c) = (b.begin as usize, b.end as usize);
            if a >= n || c >= n || a == c || edge[a * n + c] != NONE {
                return Err("requires a simple graph with valid distinct bond endpoints".into());
            }
            edge[a * n + c] = i;
            edge[c * n + a] = i;
            adj[a].push((c, i));
            adj[c].push((a, i));
            for s in [b.stereo_atom1, b.stereo_atom2].into_iter().flatten() {
                if s as usize >= n {
                    return Err("invalid stereo atom index".into());
                }
            }
            if b.stereo != EzStereo::None {
                stereo_bonds.push(i);
                for s in [b.stereo_atom1, b.stereo_atom2].into_iter().flatten() {
                    stereo_designated[s as usize] = true;
                }
            }
        }
        // Linear-time bridge detection, iterative to avoid graph-sized recursion.
        let mut time = 0;
        let mut tin = vec![NONE; n];
        let mut low = vec![0; n];
        let mut parent = vec![NONE; n];
        let mut parent_edge = vec![NONE; n];
        let mut next = vec![0; n];
        let mut bridge = vec![false; mol.n_bonds()];
        for root in 0..n {
            if tin[root] != NONE {
                continue;
            }
            tin[root] = time;
            low[root] = time;
            time += 1;
            let mut stack = vec![root];
            while let Some(&v) = stack.last() {
                if next[v] < adj[v].len() {
                    let (w, e) = adj[v][next[v]];
                    next[v] += 1;
                    if e == parent_edge[v] {
                        continue;
                    }
                    if tin[w] == NONE {
                        parent[w] = v;
                        parent_edge[w] = e;
                        tin[w] = time;
                        low[w] = time;
                        time += 1;
                        stack.push(w);
                    } else {
                        low[v] = low[v].min(tin[w]);
                    }
                } else {
                    stack.pop();
                    if parent[v] != NONE {
                        let p = parent[v];
                        bridge[parent_edge[v]] = low[v] > tin[p];
                        low[p] = low[p].min(low[v]);
                    }
                }
            }
        }
        let mut ring = vec![false; n];
        for (i, b) in mol.bonds.iter().enumerate() {
            if !bridge[i] {
                ring[b.begin as usize] = true;
                ring[b.end as usize] = true;
            }
        }
        Ok(Self {
            mol,
            n,
            edge,
            adj,
            ring,
            stereo_bonds,
            stereo_designated,
        })
    }
    /// Full atom type for seed rarity: element, charge, radical, ring
    /// membership, degree, and aromatic incidence.
    fn atom_type(&self, a: usize) -> AtomType {
        let x = &self.mol.atoms[a];
        let aromatic = self.adj[a].iter().any(|&(_, e)| self.mol.bonds[e].aromatic);
        (
            x.atomic_number,
            x.formal_charge,
            x.radical,
            self.ring[a],
            self.adj[a].len(),
            aromatic,
        )
    }
    fn bond(&self, a: usize, b: usize) -> usize {
        self.edge[a * self.n + b]
    }
}
fn atom_cost(q: &Graph<'_>, r: &Graph<'_>, a: usize, b: usize) -> i64 {
    let (x, y) = (&q.mol.atoms[a], &r.mol.atoms[b]);
    i64::from(COST_ELEMENT) * i64::from(x.atomic_number != y.atomic_number)
        + i64::from(COST_CHARGE_RADICAL)
            * i64::from(x.formal_charge != y.formal_charge || x.radical != y.radical)
        + i64::from(COST_RING) * i64::from(q.ring[a] != r.ring[b])
}
/// Unmatched atoms and bonds cost on BOTH sides, so matching a pair saves
/// two unmatched penalties (one query, one reference) minus its mismatch.
const PAIR_GAIN: i64 = 2 * COST_UNMATCHED as i64;
/// Cost when nothing maps: every query and reference atom and bond unmatched.
fn unmatched_baseline(query: &MolGraph, reference: &MolGraph) -> i64 {
    i64::from(COST_UNMATCHED)
        * (query.n_atoms() + query.n_bonds() + reference.n_atoms() + reference.n_bonds()) as i64
}
fn bond_gain(a: &BondFeat, b: &BondFeat) -> i64 {
    PAIR_GAIN
        - if (a.aromatic && b.aromatic) || a.order == b.order {
            0
        } else {
            i64::from(COST_BOND_ORDER)
        }
}

/// All side constraints are conjunctive. Missing endpoints/substituents defer
/// their constraints. Adding atoms cannot repair a violated constraint.
fn stereo_ok(q: &Graph<'_>, r: &Graph<'_>, map: &[usize]) -> bool {
    for &i in &q.stereo_bonds {
        let qb = &q.mol.bonds[i];
        let (a, b) = (map[qb.begin as usize], map[qb.end as usize]);
        if a == NONE || b == NONE {
            continue;
        }
        let e = r.bond(a, b);
        if e == NONE {
            continue;
        }
        let rb = &r.mol.bonds[e];
        if rb.stereo == EzStereo::None {
            continue;
        }
        let (Some(q1), Some(q2), Some(r1), Some(r2)) = (
            qb.stereo_atom1,
            qb.stereo_atom2,
            rb.stereo_atom1,
            rb.stereo_atom2,
        ) else {
            continue;
        };
        let wants = if a == rb.begin as usize {
            [r1, r2]
        } else {
            [r2, r1]
        };
        for (qa, want) in [q1, q2].into_iter().zip(wants) {
            let got = map[qa as usize];
            if got == NONE {
                continue;
            }
            if qb.stereo != rb.stereo {
                if got != a && got != b && (r.bond(got, a) != NONE || r.bond(got, b) != NONE) {
                    return false;
                }
            } else if got != want as usize && r.stereo_designated[got] {
                return false;
            }
        }
    }
    true
}

// Permutations act as p[old] = new. compose(a,b) means a after b.
type Perm = Vec<usize>;
fn compose(a: &[usize], b: &[usize]) -> Perm {
    b.iter().map(|&v| a[v]).collect()
}
fn inverse(p: &[usize]) -> Perm {
    let mut inv = vec![0; p.len()];
    for (i, &v) in p.iter().enumerate() {
        inv[v] = i;
    }
    inv
}
#[derive(Clone)]
struct Group {
    n: usize,
    generators: Vec<Perm>,
}
impl Group {
    fn new(n: usize, generators: Vec<Perm>) -> Self {
        let mut seen = HashSet::new();
        let generators = generators
            .into_iter()
            .filter(|p| p.iter().enumerate().any(|(i, &v)| i != v) && seen.insert(p.clone()))
            .collect();
        Self { n, generators }
    }
    fn orbit(&self, root: usize) -> Vec<usize> {
        let mut seen = vec![false; self.n];
        seen[root] = true;
        let mut orbit = vec![root];
        let mut i = 0;
        while i < orbit.len() {
            let v = orbit[i];
            for p in &self.generators {
                if !seen[p[v]] {
                    seen[p[v]] = true;
                    orbit.push(p[v]);
                }
            }
            i += 1;
        }
        orbit
    }
    /// `rep[v]` = the representative (lowest index) of `v`'s orbit.
    fn orbit_representative_of(&self) -> Vec<usize> {
        let mut rep = vec![NONE; self.n];
        for v in 0..self.n {
            if rep[v] == NONE {
                for w in self.orbit(v) {
                    rep[w] = v;
                }
            }
        }
        rep
    }
    fn representatives(&self) -> Vec<usize> {
        let mut seen = vec![false; self.n];
        let mut reps = Vec::new();
        for v in 0..self.n {
            if !seen[v] {
                reps.push(v);
                for w in self.orbit(v) {
                    seen[w] = true;
                }
            }
        }
        reps
    }
    // Schreier lemma: t[g(x)]^-1 * g * t[x] fixes root. This generates
    // the FULL point stabilizer of the supplied subgroup, not just the subset
    // of original generators that happen to fix root.
    fn stabilizer(&self, root: usize) -> Self {
        if self.generators.iter().all(|g| g[root] == root) {
            return self.clone();
        }
        let mut transversal: Vec<Option<Perm>> = vec![None; self.n];
        transversal[root] = Some((0..self.n).collect());
        let mut queue = VecDeque::from([root]);
        let mut orbit = Vec::new();
        while let Some(x) = queue.pop_front() {
            orbit.push(x);
            for g in &self.generators {
                let y = g[x];
                if transversal[y].is_none() {
                    transversal[y] = Some(compose(g, transversal[x].as_ref().unwrap()));
                    queue.push_back(y);
                }
            }
        }
        let inverses: Vec<_> = transversal
            .iter()
            .map(|t| t.as_ref().map(|p| inverse(p)))
            .collect();
        let mut gens = Vec::new();
        for x in orbit {
            for g in &self.generators {
                let p = compose(
                    inverses[g[x]].as_ref().unwrap(),
                    &compose(g, transversal[x].as_ref().unwrap()),
                );
                gens.push(p);
            }
        }
        Self::new(self.n, gens)
    }
}
fn reference_group(r: &Graph<'_>, generators: &[Vec<u16>]) -> Result<Group, String> {
    let mut perms = Vec::new();
    for g in generators {
        if g.len() != r.n {
            return Err("generator must permute reference atoms only".into());
        }
        let p: Perm = g.iter().map(|&v| v as usize).collect();
        let mut seen = vec![false; r.n];
        for (a, &b) in p.iter().enumerate() {
            if b >= r.n || seen[b] {
                return Err("generator is not a permutation".into());
            }
            seen[b] = true;
            let (x, y) = (&r.mol.atoms[a], &r.mol.atoms[b]);
            if (x.atomic_number, x.formal_charge, x.radical)
                != (y.atomic_number, y.formal_charge, y.radical)
            {
                return Err("generator changes atom scoring features".into());
            }
        }
        for b in &r.mol.bonds {
            let e = r.bond(p[b.begin as usize], p[b.end as usize]);
            if e == NONE {
                return Err("generator changes connectivity".into());
            }
            let c = &r.mol.bonds[e];
            if (b.order, b.aromatic) != (c.order, c.aromatic) {
                return Err("generator changes bond scoring features".into());
            }
        }
        perms.push(p);
    }
    let mut group = Group::new(r.n, perms);
    let mut fixed = vec![false; r.n];
    for &e in &r.stereo_bonds {
        let b = &r.mol.bonds[e];
        for v in [Some(b.begin), Some(b.end), b.stereo_atom1, b.stereo_atom2]
            .into_iter()
            .flatten()
        {
            fixed[v as usize] = true;
        }
    }
    for (v, fix) in fixed.into_iter().enumerate() {
        if fix {
            group = group.stabilizer(v);
        }
    }
    Ok(group)
}

// Maximum-weight optional matching. Real weights are nonnegative optimistic
// scores; one zero-weight dummy column per row allows every row to be omitted.
// Rectangular Hungarian algorithm, using negative weights as minimization costs.
fn assignment_upper(w: &[Vec<i64>]) -> i64 {
    let n = w.len();
    if n == 0 {
        return 0;
    }
    let real = w[0].len();
    let m = real + n;
    let mut u = vec![0i64; n + 1];
    let mut v = vec![0i64; m + 1];
    let mut p = vec![0usize; m + 1];
    let mut way = vec![0usize; m + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![i64::MAX / 4; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let (mut delta, mut j1) = (i64::MAX / 4, 0);
            for j in 1..=m {
                if used[j] {
                    continue;
                }
                let cost = if j <= real { -w[i0 - 1][j - 1] } else { 0 };
                let cur = cost - u[i0] - v[j];
                if cur < minv[j] {
                    minv[j] = cur;
                    way[j] = j0;
                }
                if minv[j] < delta {
                    delta = minv[j];
                    j1 = j;
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    (1..=real)
        .filter(|&j| p[j] != 0)
        .map(|j| w[p[j] - 1][j - 1])
        .sum()
}

struct Search<'a> {
    q: Graph<'a>,
    r: Graph<'a>,
    atom_gain: Vec<i64>,
    map: Vec<usize>,
    inv: Vec<usize>,
    // Bitset per query atom, with a single rollback trail.
    forbidden: Vec<Vec<u64>>,
    trail: Vec<(usize, usize)>,
    rank: Vec<usize>,
    root_rank: usize,
    score: i64,
    best_score: i64,
    best_map: Vec<usize>,
    stats: SearchStats,
    limits: SearchLimits,
}
impl Search<'_> {
    fn budget_exhausted(&mut self) -> bool {
        if self.stats.timed_out {
            return true;
        }
        if let Some(max) = self.limits.max_states {
            if self.stats.states >= max {
                self.stats.timed_out = true;
                return true;
            }
        }
        if let Some(deadline) = self.limits.deadline {
            if self.stats.states % 32 == 0 && Instant::now() >= deadline {
                self.stats.timed_out = true;
                return true;
            }
        }
        false
    }
    fn allowed(&self, a: usize, b: usize) -> bool {
        self.map[a] == NONE
            && self.inv[b] == NONE
            && self.rank[a] >= self.root_rank
            && self.forbidden[a][b / 64] & (1u64 << (b % 64)) == 0
    }
    fn forbid(&mut self, a: usize, b: usize) {
        let bit = 1u64 << (b % 64);
        if self.forbidden[a][b / 64] & bit == 0 {
            self.forbidden[a][b / 64] |= bit;
            self.trail.push((a, b));
        }
    }
    fn rollback(&mut self, mark: usize) {
        while self.trail.len() > mark {
            let (a, b) = self.trail.pop().unwrap();
            self.forbidden[a][b / 64] &= !(1u64 << (b % 64));
        }
    }
    fn delta(&self, a: usize, b: usize) -> (i64, bool) {
        let mut gain = self.atom_gain[a * self.r.n + b];
        let mut attached = false;
        for &(x, e) in &self.q.adj[a] {
            if self.map[x] != NONE {
                let f = self.r.bond(b, self.map[x]);
                if f != NONE {
                    attached = true;
                    gain += bond_gain(&self.q.mol.bonds[e], &self.r.mol.bonds[f]);
                }
            }
        }
        (gain, attached)
    }
    fn update_best(&mut self) {
        if self.score > self.best_score {
            self.best_score = self.score;
            self.best_map.copy_from_slice(&self.map);
        }
    }
    // Returns true only if the branch can strictly improve the incumbent.
    fn can_improve(&mut self) -> bool {
        let qs: Vec<_> = (0..self.q.n)
            .filter(|&a| self.map[a] == NONE && self.rank[a] >= self.root_rank)
            .collect();
        let rs: Vec<_> = (0..self.r.n).filter(|&b| self.inv[b] == NONE).collect();
        if qs.is_empty() || rs.is_empty() {
            return false;
        }
        let mut weights = vec![vec![0; rs.len()]; qs.len()];
        let mut row_bound = 0;
        for (i, &a) in qs.iter().enumerate() {
            for (j, &b) in rs.iter().enumerate() {
                if !self.allowed(a, b) {
                    continue;
                }
                let mut w2 = 2 * self.delta(a, b).0;
                for &(x, e) in &self.q.adj[a] {
                    if self.map[x] != NONE || self.rank[x] < self.root_rank {
                        continue;
                    }
                    let mut best = 0;
                    for &(y, f) in &self.r.adj[b] {
                        if self.allowed(x, y) {
                            best = best.max(bond_gain(&self.q.mol.bonds[e], &self.r.mol.bonds[f]));
                        }
                    }
                    w2 += best;
                }
                // Clipping negatives and allowing zero-weight forbidden cells
                // only relax the assignment problem, so the bound stays safe.
                weights[i][j] = w2.max(0);
            }
            row_bound += weights[i].iter().copied().max().unwrap_or(0);
        }
        let needed = 2 * (self.best_score - self.score);
        if row_bound <= needed {
            return false;
        }
        let col_bound: i64 = (0..rs.len())
            .map(|j| weights.iter().map(|row| row[j]).max().unwrap_or(0))
            .sum();
        if col_bound <= needed {
            return false;
        }
        self.stats.assignment_bounds += 1;
        assignment_upper(&weights) > needed
    }
    fn choose_pair(&mut self) -> Option<(usize, usize, i64)> {
        let mut best: Option<(usize, usize, i64, usize)> = None;
        for a in 0..self.q.n {
            if self.map[a] != NONE || self.rank[a] < self.root_rank {
                continue;
            }
            let mut candidates = Vec::new();
            for b in 0..self.r.n {
                if !self.allowed(a, b) {
                    continue;
                }
                let (gain, attached) = self.delta(a, b);
                if !attached {
                    continue;
                }
                self.map[a] = b;
                let valid = stereo_ok(&self.q, &self.r, &self.map);
                self.map[a] = NONE;
                if valid {
                    candidates.push((b, gain));
                }
            }
            let count = candidates.len();
            for (b, gain) in candidates {
                let replace = match best {
                    None => true,
                    Some((ba, bb, bg, bc)) => {
                        gain > bg
                            || (gain == bg && (count, self.rank[a], b) < (bc, self.rank[ba], bb))
                    }
                };
                if replace {
                    best = Some((a, b, gain, count));
                }
            }
        }
        best.map(|(a, b, gain, _)| (a, b, gain))
    }
    fn dfs(&mut self, group: &Group) {
        self.update_best();
        let mark = self.trail.len();
        // Exclusion branches use a loop: call-stack depth is at most the number
        // of mapped atoms, not the number of rejected atom pairs.
        loop {
            self.stats.states += 1;
            if self.budget_exhausted() {
                break;
            }
            if !self.can_improve() {
                self.stats.bound_prunes += 1;
                break;
            }
            let Some((a, b, gain)) = self.choose_pair() else {
                break;
            };
            let orbit = group.orbit(b);
            debug_assert!(orbit.iter().all(|&v| self.allowed(a, v)));
            self.stats.symmetry_alternatives_removed += (orbit.len() - 1) as u64;
            let child_group = group.stabilizer(b);
            self.map[a] = b;
            self.inv[b] = a;
            self.score += gain;
            self.dfs(&child_group);
            self.score -= gain;
            self.inv[b] = NONE;
            self.map[a] = NONE;
            if self.stats.timed_out {
                break;
            }
            // Excluding the WHOLE orbit preserves group invariance of every
            // forbidden row. No partition stabilizer or nauty rerun is needed.
            for v in orbit {
                self.forbid(a, v);
            }
        }
        self.rollback(mark);
    }
    fn greedy(&mut self) {
        let mut added = Vec::new();
        while let Some((a, b, gain)) = self.choose_pair() {
            self.stats.states += 1;
            if self.budget_exhausted() {
                break;
            }
            if gain <= 0 {
                break;
            } // Only incumbent initialization is greedy.
            self.map[a] = b;
            self.inv[b] = a;
            self.score += gain;
            added.push((a, b, gain));
            self.update_best();
        }
        for (a, b, gain) in added.into_iter().rev() {
            self.map[a] = NONE;
            self.inv[b] = NONE;
            self.score -= gain;
        }
    }

    fn clear_map(&mut self) {
        self.map.fill(NONE);
        self.inv.fill(NONE);
        self.score = 0;
        self.root_rank = 0;
    }

    fn apply_pairs(&mut self, pairs: &[(usize, usize)]) -> bool {
        for (i, &(a, b)) in pairs.iter().enumerate() {
            if !self.allowed(a, b) {
                for &(x, y) in &pairs[..i] {
                    self.map[x] = NONE;
                    self.inv[y] = NONE;
                }
                self.score = 0;
                return false;
            }
            let (gain, _) = self.delta(a, b);
            self.map[a] = b;
            self.inv[b] = a;
            self.score += gain;
            if !stereo_ok(&self.q, &self.r, &self.map) {
                for &(x, y) in &pairs[..=i] {
                    self.map[x] = NONE;
                    self.inv[y] = NONE;
                }
                self.score = 0;
                return false;
            }
        }
        true
    }

    fn search_from_partial(&mut self, pairs: &[(usize, usize)], group: &Group) {
        if pairs.is_empty() || self.budget_exhausted() {
            return;
        }
        self.root_rank = 0;
        if !self.apply_pairs(pairs) {
            return;
        }
        self.stats.seeds += 1;
        self.update_best();
        self.greedy();
        if !self.limits.greedy_only && !self.stats.timed_out {
            let mut g = group.clone();
            for &(_, b) in pairs {
                g = g.stabilizer(b);
            }
            self.dfs(&g);
        }
        self.clear_map();
    }
}

fn relevant_ring_cycles(g: &Graph<'_>, max_size: usize) -> Vec<Vec<usize>> {
    if g.n == 0 || g.mol.bonds.is_empty() {
        return Vec::new();
    }
    let mut adj: HashMap<i32, Vec<i32>> = (0..g.n as i32).map(|i| (i, Vec::new())).collect();
    let mut bonds = Vec::with_capacity(g.mol.bonds.len());
    for b in &g.mol.bonds {
        let u = i32::from(b.begin);
        let v = i32::from(b.end);
        adj.entry(u).or_default().push(v);
        adj.entry(v).or_default().push(u);
        bonds.push((u, v));
    }
    find_relevant_rings_from_adj(&adj, bonds, max_size)
        .into_iter()
        .map(|ring| ring.atoms.into_iter().map(|a| a as usize).collect())
        .collect()
}

fn cyclic_ring_pairs(q_ring: &[usize], r_ring: &[usize]) -> Vec<Vec<(usize, usize)>> {
    let n = q_ring.len();
    if n == 0 || n != r_ring.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(2 * n);
    for k in 0..n {
        for flip in [false, true] {
            let mut pairs = Vec::with_capacity(n);
            for i in 0..n {
                let b = if flip {
                    r_ring[(k + n - i) % n]
                } else {
                    r_ring[(i + k) % n]
                };
                pairs.push((q_ring[i], b));
            }
            out.push(pairs);
        }
    }
    out
}

fn ring_map_atom_score(pairs: &[(usize, usize)], atom_gain: &[i64], nr: usize) -> Option<i64> {
    let mut score = 0i64;
    for &(a, b) in pairs {
        let g = atom_gain[a * nr + b];
        if g <= 0 {
            return None;
        }
        score += g;
    }
    Some(score)
}

fn pack_nonconflicting(cands: &[(i64, Vec<(usize, usize)>)]) -> Vec<(usize, usize)> {
    let mut used_q = HashSet::new();
    let mut used_r = HashSet::new();
    let mut out = Vec::new();
    for (_, pairs) in cands {
        if pairs
            .iter()
            .any(|&(a, b)| used_q.contains(&a) || used_r.contains(&b))
        {
            continue;
        }
        for &(a, b) in pairs {
            used_q.insert(a);
            used_r.insert(b);
            out.push((a, b));
        }
    }
    out
}

/// Equal-size Relevant Ring cyclic maps, unique ring sizes first. Search-order only.
fn ring_seed_partials(q: &Graph<'_>, r: &Graph<'_>, atom_gain: &[i64]) -> Vec<Vec<(usize, usize)>> {
    let q_rings = relevant_ring_cycles(q, ALIGN_RING_MAX);
    let r_rings = relevant_ring_cycles(r, ALIGN_RING_MAX);
    if q_rings.is_empty() || r_rings.is_empty() {
        return Vec::new();
    }
    let q_size_n = |sz: usize| q_rings.iter().filter(|ring| ring.len() == sz).count();
    let r_size_n = |sz: usize| r_rings.iter().filter(|ring| ring.len() == sz).count();
    let unique: HashSet<usize> = q_rings
        .iter()
        .map(|ring| ring.len())
        .filter(|&sz| q_size_n(sz) == 1 && r_size_n(sz) == 1)
        .collect();
    let max_eq = q_rings
        .iter()
        .map(|ring| ring.len())
        .filter(|&sz| r_size_n(sz) > 0)
        .max();
    let use_size = |sz: usize| {
        if unique.is_empty() {
            Some(sz) == max_eq
        } else {
            unique.contains(&sz)
        }
    };

    let mut scored: Vec<(i64, Vec<(usize, usize)>)> = Vec::new();
    let mut seen = HashSet::new();
    for qr in &q_rings {
        if !use_size(qr.len()) {
            continue;
        }
        for rr in &r_rings {
            if rr.len() != qr.len() {
                continue;
            }
            for pairs in cyclic_ring_pairs(qr, rr) {
                let Some(score) = ring_map_atom_score(&pairs, atom_gain, r.n) else {
                    continue;
                };
                let mut key: Vec<(usize, usize)> = pairs.clone();
                key.sort_unstable();
                if !seen.insert(key) {
                    continue;
                }
                scored.push((score, pairs));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.len().cmp(&a.1.len())));

    let mut out = Vec::new();
    let packed = pack_nonconflicting(&scored);
    if packed.len() > scored.first().map(|c| c.1.len()).unwrap_or(0) {
        out.push(packed);
    }
    for (_, pairs) in scored.into_iter().take(MAX_RING_SEEDS) {
        out.push(pairs);
        if out.len() >= MAX_RING_SEEDS {
            break;
        }
    }
    out
}

/// Exact, nonempty, connected map. Empty input graphs return None.
/// Reference symmetry comes from [`reference_generators`] (Canonaut).
/// Panics on malformed graphs; use cost_mapping_with_generators for errors.
pub fn cost_mapping(reference: &MolGraph, query: &MolGraph) -> Option<MappingResult> {
    let generators = reference_generators(reference);
    cost_mapping_with_generators(reference, query, &generators)
        .expect("invalid molecular graph")
        .0
}

thread_local! {
    static CANONAUT_GENERATORS: std::cell::RefCell<Vec<Vec<u32>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn collect_generator(_n: u32, perm: &mut [u32], _: &mut [u32], _: u32, _: u32, _: u32) {
    CANONAUT_GENERATORS.with(|g| g.borrow_mut().push(perm.to_vec()));
}

/// Atom-permutation generators of `mol`'s automorphism group, via Canonaut on
/// the atom–bond incidence graph. Atom vertices are colored by
/// (element, charge, radical) and bond vertices by (order, aromatic), so every
/// generator preserves exactly the features [`cost_mapping_with_generators`]
/// checks. Generators are projected onto atom vertices.
pub fn reference_generators(mol: &MolGraph) -> Vec<Vec<u16>> {
    use canonaut::structs::{CanonautManager, DenseGraph};
    let (n, m) = (mol.n_atoms(), mol.n_bonds());
    if n < 2 {
        return Vec::new();
    }
    let mut g = DenseGraph::new(n + m);
    for (i, b) in mol.bonds.iter().enumerate() {
        g.add_edge(b.begin as usize, n + i);
        g.add_edge(b.end as usize, n + i);
    }
    let mut keys: Vec<(u8, u8, i8, u8, bool)> = Vec::with_capacity(n + m);
    keys.extend(
        mol.atoms
            .iter()
            .map(|a| (0, a.atomic_number, a.formal_charge, a.radical, false)),
    );
    keys.extend(mol.bonds.iter().map(|b| (1, b.order, 0, 0, b.aromatic)));
    let mut palette = keys.clone();
    palette.sort_unstable();
    palette.dedup();
    let colors: Vec<u32> = keys
        .iter()
        .map(|k| palette.binary_search(k).unwrap_or(0) as u32)
        .collect();

    CANONAUT_GENERATORS.with(|g| g.borrow_mut().clear());
    let mut manager = CanonautManager::new(n + m).with_automorphism_callback(collect_generator);
    manager.canonize_graph_with_colors(&g, &colors);
    let perms = CANONAUT_GENERATORS.with(|g| std::mem::take(&mut *g.borrow_mut()));
    perms
        .into_iter()
        .filter(|p| p.len() >= n && p[..n].iter().all(|&v| (v as usize) < n))
        .map(|p| p[..n].iter().map(|&v| v as u16).collect())
        .collect()
}

/// Pass atom-permutation generators from Cannonaut for reference symmetry.
/// Any valid subgroup is sufficient for correctness. No full group enumeration.
/// Cost ties retain the first optimum; atom indices may affect the representative
/// returned but do not affect the optimal cost.
pub fn cost_mapping_with_generators(
    reference: &MolGraph,
    query: &MolGraph,
    reference_generators: &[Vec<u16>],
) -> Result<(Option<MappingResult>, SearchStats), String> {
    cost_mapping_with_generators_limited(
        reference,
        query,
        reference_generators,
        &SearchLimits::unbounded(),
    )
}

/// Like [`cost_mapping_with_generators`], with time / node / greedy-only caps.
/// Timeout or node cap returns the incumbent (greedy or partial BnB), not an error.
pub fn cost_mapping_with_generators_limited(
    reference: &MolGraph,
    query: &MolGraph,
    reference_generators: &[Vec<u16>],
    limits: &SearchLimits,
) -> Result<(Option<MappingResult>, SearchStats), String> {
    let q = Graph::new(query)?;
    let r = Graph::new(reference)?;
    let group = reference_group(&r, reference_generators)?;
    if q.n == 0 || r.n == 0 {
        return Ok((None, SearchStats::default()));
    }
    // Rarity is over the full atom type, pooled across both molecules.
    let q_types: Vec<_> = (0..q.n).map(|a| q.atom_type(a)).collect();
    let r_types: Vec<_> = (0..r.n).map(|b| r.atom_type(b)).collect();
    let mut frequency: HashMap<AtomType, usize> = HashMap::new();
    for t in q_types.iter().chain(&r_types) {
        *frequency.entry(*t).or_default() += 1;
    }
    let mut order: Vec<_> = (0..q.n).collect();
    order.sort_by_key(|&a| (frequency[&q_types[a]], std::cmp::Reverse(q.adj[a].len()), a));
    let mut rank = vec![0; q.n];
    for (i, &a) in order.iter().enumerate() {
        rank[a] = i;
    }
    let atom_gain: Vec<_> = (0..q.n)
        .flat_map(|a| (0..r.n).map(move |b| (a, b)))
        .map(|(a, b)| PAIR_GAIN - atom_cost(&q, &r, a, b))
        .collect();
    let mut seeds = Vec::new();
    let reference_representatives = group.representatives();
    for &a in &order {
        for &b in &reference_representatives {
            seeds.push((a, b));
        }
    }
    // Seed order only affects speed (every seed is still searched). Lead with
    // the topology-free optimal atom assignment, rarest query elements first:
    // those pairs usually sit inside the optimum, so the first incumbent is
    // strong and the bound prunes the remaining seeds early.
    let assigned = optimal_assignment(&atom_gain, q.n, r.n);
    let orbit_rep = group.orbit_representative_of();
    let mut preferred = HashSet::new();
    for (a, b) in assigned.into_iter().enumerate() {
        if let Some(b) = b {
            preferred.insert((a, orbit_rep[b]));
        }
    }
    seeds.sort_by_key(|&(a, b)| {
        let lead = preferred.contains(&(a, b));
        // Same-type pairs first, rarest type first (rank is query rarity).
        let same = q_types[a] == r_types[b];
        (
            !lead,
            !same,
            if lead { rank[a] } else { 0 },
            std::cmp::Reverse(atom_gain[a * r.n + b]),
            rank[a],
            b,
        )
    });
    let mut s = Search {
        map: vec![NONE; q.n],
        inv: vec![NONE; r.n],
        forbidden: vec![vec![0; (r.n + 63) / 64]; q.n],
        trail: Vec::new(),
        best_map: vec![NONE; q.n],
        atom_gain,
        rank,
        root_rank: 0,
        score: 0,
        best_score: i64::MIN / 4,
        stats: SearchStats::default(),
        limits: limits.clone(),
        q,
        r,
    };
    let ring_seeds = ring_seed_partials(&s.q, &s.r, &s.atom_gain);
    for pairs in &ring_seeds {
        s.search_from_partial(pairs, &group);
        if s.budget_exhausted() {
            break;
        }
        if s.best_score == unmatched_baseline(query, reference) {
            break;
        }
    }
    for (a, b) in seeds {
        if s.budget_exhausted() {
            break;
        }
        if s.best_score == unmatched_baseline(query, reference) {
            break;
        }
        // Every connected map is considered under its lowest-ranked query atom.
        // Reference orbit reduction is independent of that query restriction.
        s.root_rank = s.rank[a];
        s.map[a] = b;
        s.inv[b] = a;
        s.score = s.atom_gain[a * s.r.n + b];
        s.stats.seeds += 1;
        s.update_best();
        s.greedy();
        if !s.limits.greedy_only && !s.stats.timed_out {
            s.dfs(&group.stabilizer(b));
        }
        s.map[a] = NONE;
        s.inv[b] = NONE;
        if s.best_score == unmatched_baseline(query, reference) {
            break; // Zero cost is a global lower bound, not a search cap.
        }
    }
    if s.best_score == i64::MIN / 4 {
        return Ok((None, s.stats));
    }
    let baseline = unmatched_baseline(query, reference);
    let cost = i32::try_from(baseline - s.best_score).map_err(|_| "cost exceeds i32")?;
    let map = s
        .best_map
        .iter()
        .enumerate()
        .filter(|(_, b)| **b != NONE)
        .map(|(a, &b)| (a as u16, b as u16))
        .collect();
    Ok((Some(MappingResult { map, cost }), s.stats))
}

/// Max-gain atom assignment ignoring topology (Hungarian, O(n²·m)).
/// `gain` is row-major `nq × nr`; only positive-gain pairs are kept, so an
/// atom whose every pairing costs more than leaving it unmatched stays `None`.
fn optimal_assignment(gain: &[i64], nq: usize, nr: usize) -> Vec<Option<usize>> {
    // Square cost matrix: dummy rows/cols cost 0 (= leave unmatched).
    let n = nq.max(nr);
    let cost = |i: usize, j: usize| -> i64 {
        if i < nq && j < nr {
            -gain[i * nr + j].max(0)
        } else {
            0
        }
    };
    let (mut u, mut v) = (vec![0i64; n + 1], vec![0i64; n + 1]);
    let (mut p, mut way) = (vec![0usize; n + 1], vec![0usize; n + 1]);
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![i64::MAX; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let (i0, mut delta, mut j1) = (p[j0], i64::MAX, 0);
            for j in 1..=n {
                if !used[j] {
                    let cur = cost(i0 - 1, j - 1) - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut out = vec![None; nq];
    for j in 1..=n {
        let (i, b) = (p[j] - 1, j - 1);
        if i < nq && b < nr && gain[i * nr + b] > 0 {
            out[i] = Some(b);
        }
    }
    out
}

/// Scores an injective atom map, penalizing unmatched atoms and bonds on both
/// sides, with bonds inferred.
/// As in the original, this scorer does not enforce connectivity or E/Z validity.
pub fn mapping_cost(query: &MolGraph, reference: &MolGraph, mapping: &HashMap<u16, u16>) -> i32 {
    let q = Graph::new(query).expect("invalid query graph");
    let r = Graph::new(reference).expect("invalid reference graph");
    let mut used = vec![false; r.n];
    let mut score = 0;
    for (&a, &b) in mapping {
        let (a, b) = (a as usize, b as usize);
        assert!(
            a < q.n && b < r.n && !used[b],
            "invalid/noninjective atom map"
        );
        used[b] = true;
        score += PAIR_GAIN - atom_cost(&q, &r, a, b);
    }
    for b in &query.bonds {
        if let (Some(&x), Some(&y)) = (mapping.get(&b.begin), mapping.get(&b.end)) {
            let e = r.bond(x as usize, y as usize);
            if e != NONE {
                score += bond_gain(b, &reference.bonds[e]);
            }
        }
    }
    let baseline = unmatched_baseline(query, reference);
    i32::try_from(baseline - score).expect("cost exceeds i32")
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
            (0..n - 1)
                .map(|i| single(i as u16, (i + 1) as u16))
                .collect(),
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
    /// C1CCCCC1CCCC: ring atoms 0..5, butyl chain 6..9 on atom 5.
    fn butylcyclohexane_ring_first() -> MolGraph {
        let mut bonds: Vec<_> = (0..5).map(|i| single(i, i + 1)).collect();
        bonds.push(single(0, 5));
        bonds.extend((5..9).map(|i| single(i, i + 1)));
        MolGraph::new(vec![c(); 10], bonds)
    }
    /// CCCCC1CCCCC1: butyl chain 0..3, ring atoms 4..9.
    fn butylcyclohexane_chain_first() -> MolGraph {
        let mut bonds: Vec<_> = (0..9).map(|i| single(i, i + 1)).collect();
        bonds.push(single(4, 9));
        MolGraph::new(vec![c(); 10], bonds)
    }
    #[test]
    fn hexane_onto_butylcyclohexane_atom_order_invariant() {
        let a = cost_mapping(&chain(6), &butylcyclohexane_ring_first()).expect("map");
        let b = cost_mapping(&chain(6), &butylcyclohexane_chain_first()).expect("map");
        assert_eq!(a.map.len(), 6, "ring-first map={:?}", a.map);
        assert_eq!(b.map.len(), 6, "chain-first map={:?}", b.map);
        assert_eq!(a.cost, b.cost, "a={a:?} b={b:?}");
        // Hexane must land on the chain + ring attachment, never the ring
        // itself (ring atoms are 0..5 in ring-first order, 4..9 in chain-first).
        let ring_hits = |m: &HashMap<u16, u16>, ring: std::ops::Range<u16>| {
            m.keys().filter(|k| ring.contains(k)).count()
        };
        assert!(ring_hits(&a.map, 0..6) < 6, "ring-first map={:?}", a.map);
        assert!(ring_hits(&b.map, 4..10) < 6, "chain-first map={:?}", b.map);
        // Reverse direction too.
        let a = cost_mapping(&butylcyclohexane_ring_first(), &chain(6)).expect("map");
        let b = cost_mapping(&butylcyclohexane_chain_first(), &chain(6)).expect("map");
        assert_eq!(a.map.len(), 6, "ring-first map={:?}", a.map);
        assert_eq!(a.cost, b.cost, "a={a:?} b={b:?}");
        assert!(
            a.map.values().filter(|v| (0..6).contains(*v)).count() < 6,
            "{:?}",
            a.map
        );
        assert!(
            b.map.values().filter(|v| (4..10).contains(*v)).count() < 6,
            "{:?}",
            b.map
        );
    }
    /// Deterministic pseudo-random permutations (no rand dep).
    fn lcg_perms(n: usize, count: usize) -> Vec<Vec<u16>> {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..count)
            .map(|_| {
                let mut p: Vec<u16> = (0..n as u16).collect();
                for i in (1..n).rev() {
                    state = state
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    p.swap(i, (state >> 33) as usize % (i + 1));
                }
                p
            })
            .collect()
    }
    #[test]
    fn cost_mapping_cost_invariant_under_atom_permutation() {
        let cases = [
            (chain(6), butylcyclohexane_ring_first()),
            (butylcyclohexane_ring_first(), chain(6)),
            (chain(6), cyclohexane()),
            (cyclohexane(), butylcyclohexane_chain_first()),
        ];
        for (r, q) in cases {
            let base = cost_mapping(&r, &q).expect("map").cost;
            for pq in lcg_perms(q.n_atoms(), 40) {
                for pr in lcg_perms(r.n_atoms(), 3) {
                    let got = cost_mapping(&r.permute(&pr), &q.permute(&pq))
                        .expect("map")
                        .cost;
                    assert_eq!(got, base, "pq={pq:?} pr={pr:?}");
                }
            }
        }
    }
    #[test]
    fn hexane_maps_all_cyclohexane_atoms() {
        let res = cost_mapping(&chain(6), &cyclohexane()).expect("map");
        assert_eq!(res.map.len(), 6, "map={:?}", res.map);
        // One unmatched ring-closure bond (cost > any pair mismatch).
        assert_eq!(
            res.cost,
            COST_UNMATCHED + 6 * COST_RING,
            "cost={}",
            res.cost
        );
    }
    #[test]
    fn costly_terminal_element_maps_when_both_sides_pay() {
        // C–N onto C–C: N→C (11) beats leaving N, C, and both bonds unmatched (20).
        let mut a = chain(2);
        a.atoms[1].atomic_number = 7;
        let res = cost_mapping(&chain(2), &a).expect("map");
        assert_eq!(res.map.len(), 2);
        assert_eq!(res.cost, COST_ELEMENT);
    }
    fn aromatic(a: u16, b: u16) -> BondFeat {
        BondFeat {
            begin: a,
            end: b,
            order: 4,
            aromatic: true,
            stereo: EzStereo::None,
            stereo_atom1: None,
            stereo_atom2: None,
        }
    }
    #[test]
    fn aromatic_ring_beats_substituent_element_mismatch() {
        // o-cresol-like: ring 0..5, OH on 0, Me on 1.
        let mut cresol_atoms = vec![c(); 8];
        cresol_atoms[6].atomic_number = 8; // OH
                                           // 7 = Me carbon
        let cresol = MolGraph::new(
            cresol_atoms,
            vec![
                aromatic(0, 1),
                aromatic(1, 2),
                aromatic(2, 3),
                aromatic(3, 4),
                aromatic(4, 5),
                aromatic(5, 0),
                single(0, 6),
                single(1, 7),
            ],
        );
        // anisole-like: Me(6)-O(7)-ring 0..5
        let mut anisole_atoms = vec![c(); 8];
        anisole_atoms[7].atomic_number = 8;
        let anisole = MolGraph::new(
            anisole_atoms,
            vec![
                aromatic(0, 1),
                aromatic(1, 2),
                aromatic(2, 3),
                aromatic(3, 4),
                aromatic(4, 5),
                aromatic(5, 0),
                single(0, 7),
                single(7, 6),
            ],
        );
        let res = cost_mapping(&cresol, &anisole).expect("map");
        // Ring carbons must map C→C; oxygen must not map onto carbon.
        for (&q, &r) in &res.map {
            let zq = anisole.atoms[q as usize].atomic_number;
            let zr = cresol.atoms[r as usize].atomic_number;
            assert_eq!(
                zq, zr,
                "element mismatch in map q{q}(Z={zq})→r{r}(Z={zr}); map={:?}",
                res.map
            );
        }
        let ring_pairs = res.map.iter().filter(|(&q, _)| q < 6).count();
        assert!(
            ring_pairs >= 6,
            "expected full aromatic ring map, got {ring_pairs}; map={:?}",
            res.map
        );
    }
    #[test]
    fn toluene_anisole_ring_map_is_cheaper_than_shifted_methyl() {
        // Chematic-like: anisole Me(0)-O(1)-ring 2..7; toluene Me(0)-ring 1..6.
        let mut anisole_atoms = vec![c(); 8];
        anisole_atoms[1].atomic_number = 8;
        let anisole = MolGraph::new(
            anisole_atoms,
            vec![
                single(0, 1),
                single(1, 2),
                aromatic(2, 3),
                aromatic(3, 4),
                aromatic(4, 5),
                aromatic(5, 6),
                aromatic(6, 7),
                aromatic(7, 2),
            ],
        );
        let toluene = MolGraph::new(
            vec![c(); 7],
            vec![
                single(0, 1),
                aromatic(1, 2),
                aromatic(2, 3),
                aromatic(3, 4),
                aromatic(4, 5),
                aromatic(5, 6),
                aromatic(6, 1),
            ],
        );
        // Good: full ring + pendant Me↔O (both exo to ipso). Bad: methyl steals a ring slot.
        let good: HashMap<u16, u16> = [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7)]
            .into_iter()
            .collect();
        let bad: HashMap<u16, u16> = [(0, 3), (1, 2), (2, 7), (3, 6), (4, 5), (5, 4)]
            .into_iter()
            .collect();
        let good_c = mapping_cost(&toluene, &anisole, &good);
        let bad_c = mapping_cost(&toluene, &anisole, &bad);
        assert!(
            good_c < bad_c,
            "ring+Me↔O map should beat shifted map: good={good_c} bad={bad_c}"
        );
        let res = cost_mapping(&anisole, &toluene).expect("map");
        assert!(
            res.cost <= good_c,
            "search returned worse than known Me↔O ring map: got {} (map={:?}) vs good {good_c}",
            res.cost,
            res.map
        );
        // Leaving the methyl unmatched would also strand anisole's O and
        // its bond (20), so Me→O (11) is the minimum.
        assert_eq!(res.map.get(&0), Some(&1));
        // Plus anisole's methyl carbon and its O–C bond left unmatched.
        assert_eq!(res.cost, COST_ELEMENT + 2 * COST_UNMATCHED);
        let ring_pairs = res
            .map
            .iter()
            .filter(|(&q, &r)| q >= 1 && q <= 6 && r >= 2 && r <= 7)
            .count();
        assert!(
            ring_pairs >= 6,
            "expected full aromatic ring map, got {ring_pairs}; map={:?}",
            res.map
        );
    }
    #[test]
    fn ez_self_all_orderings_zero_cost() {
        for g in [
            alkene(true),
            alkene(false),
            hetero_alkene(true),
            branched_alkene(true),
        ] {
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

#[cfg(test)]
mod exact_tests {
    use super::*;
    fn atom(z: u8) -> AtomFeat {
        AtomFeat {
            atomic_number: z,
            formal_charge: 0,
            radical: 0,
            chirality: AtomChirality::None,
        }
    }
    fn edge(a: usize, b: usize) -> BondFeat {
        BondFeat {
            begin: a as u16,
            end: b as u16,
            order: 1,
            aromatic: false,
            stereo: EzStereo::None,
            stereo_atom1: None,
            stereo_atom2: None,
        }
    }
    fn graph(n: usize, bits: usize) -> MolGraph {
        let mut bonds = Vec::new();
        let mut bit = 0;
        for a in 0..n {
            for b in 0..a {
                if bits & (1 << bit) != 0 {
                    bonds.push(edge(a, b));
                }
                bit += 1;
            }
        }
        MolGraph::new(vec![atom(6); n], bonds)
    }
    fn connected(q: &Graph<'_>, r: &Graph<'_>, map: &[usize]) -> bool {
        let Some(root) = map.iter().position(|&b| b != NONE) else {
            return false;
        };
        let mut seen = vec![false; q.n];
        seen[root] = true;
        let mut stack = vec![root];
        while let Some(a) = stack.pop() {
            for &(x, _) in &q.adj[a] {
                if !seen[x] && map[x] != NONE && r.bond(map[a], map[x]) != NONE {
                    seen[x] = true;
                    stack.push(x);
                }
            }
        }
        map.iter().enumerate().all(|(a, &b)| b == NONE || seen[a])
    }
    // Independent exhaustive partial-injection enumeration: no bounds, seeds,
    // frontier ordering, or symmetry pruning. Scoring counts penalties directly.
    fn brute(reference: &MolGraph, query: &MolGraph) -> i32 {
        let q = Graph::new(query).unwrap();
        let r = Graph::new(reference).unwrap();
        fn rec(
            q: &Graph<'_>,
            r: &Graph<'_>,
            map: &mut [usize],
            used: &mut [bool],
            a: usize,
            best: &mut i32,
        ) {
            if a < q.n {
                map[a] = NONE;
                rec(q, r, map, used, a + 1, best);
                for b in 0..r.n {
                    if !used[b] {
                        used[b] = true;
                        map[a] = b;
                        rec(q, r, map, used, a + 1, best);
                        used[b] = false;
                    }
                }
                map[a] = NONE;
                return;
            }
            if !connected(q, r, map) || !stereo_ok(q, r, map) {
                return;
            }
            let mut cost = 0;
            let mut matched_bonds = 0;
            for (a, &b) in map.iter().enumerate() {
                cost += if b == NONE {
                    COST_UNMATCHED
                } else {
                    atom_cost(q, r, a, b) as i32
                };
            }
            for qb in &q.mol.bonds {
                let (x, y) = (map[qb.begin as usize], map[qb.end as usize]);
                let e = if x != NONE && y != NONE {
                    r.bond(x, y)
                } else {
                    NONE
                };
                cost += if e == NONE {
                    COST_UNMATCHED
                } else {
                    matched_bonds += 1;
                    (PAIR_GAIN - bond_gain(qb, &r.mol.bonds[e])) as i32
                };
            }
            // Reference-side leftovers cost too.
            let matched_atoms = map.iter().filter(|&&b| b != NONE).count();
            cost += COST_UNMATCHED * (r.n - matched_atoms + r.mol.n_bonds() - matched_bonds) as i32;
            *best = (*best).min(cost);
        }
        let mut best = i32::MAX;
        rec(
            &q,
            &r,
            &mut vec![NONE; q.n],
            &mut vec![false; r.n],
            0,
            &mut best,
        );
        best
    }
    fn assert_solution(r: &MolGraph, q: &MolGraph, gens: &[Vec<u16>]) {
        let result = cost_mapping_with_generators(r, q, gens).unwrap().0.unwrap();
        assert_eq!(
            result.cost,
            brute(r, q),
            "q={q:?} r={r:?} map={:?}",
            result.map
        );
        assert_eq!(result.cost, mapping_cost(q, r, &result.map));
        let mut dense = vec![NONE; q.n_atoms()];
        for (&a, &b) in &result.map {
            dense[a as usize] = b as usize;
        }
        let (qg, rg) = (Graph::new(q).unwrap(), Graph::new(r).unwrap());
        assert!(connected(&qg, &rg, &dense));
        assert!(stereo_ok(&qg, &rg, &dense));
    }
    #[test]
    fn exhaustive_all_pairs_of_four_vertex_graphs() {
        for qb in 0..64 {
            for rb in 0..64 {
                assert_solution(&graph(4, rb), &graph(4, qb), &[]);
            }
        }
    }
    #[test]
    fn weighted_random_graphs_match_brute_force() {
        let mut state = 1234567u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            state >> 32
        };
        for _ in 0..160 {
            let mut q = graph(5, next() as usize & 1023);
            let mut r = graph(5, next() as usize & 1023);
            for mol in [&mut q, &mut r] {
                for a in &mut mol.atoms {
                    a.atomic_number = [6, 6, 7, 8][next() as usize % 4];
                    a.formal_charge = (next() % 3) as i8 - 1;
                }
                for b in &mut mol.bonds {
                    b.order = (next() % 3 + 1) as u8;
                }
            }
            assert_solution(&r, &q, &[]);
        }
    }
    fn closure(g: &Group) -> HashSet<Perm> {
        let id: Perm = (0..g.n).collect();
        let mut seen = HashSet::from([id.clone()]);
        let mut queue = VecDeque::from([id]);
        while let Some(p) = queue.pop_front() {
            for a in &g.generators {
                let b = compose(a, &p);
                if seen.insert(b.clone()) {
                    queue.push_back(b);
                }
            }
        }
        seen
    }
    #[test]
    fn schreier_recovers_stabilizer_compositions() {
        let g = Group::new(4, vec![vec![1, 2, 3, 0], vec![1, 0, 2, 3]]);
        assert_eq!(closure(&g).len(), 24);
        let fixed = g.stabilizer(0);
        let expected: HashSet<_> = closure(&g).into_iter().filter(|p| p[0] == 0).collect();
        assert_eq!(closure(&fixed), expected);
        assert_eq!(closure(&fixed).len(), 6);
        assert_eq!(closure(&fixed.stabilizer(1)).len(), 2);
    }
    #[test]
    fn symmetry_and_exclusions_match_brute_force() {
        let ring = MolGraph::new(
            vec![atom(6); 5],
            (0..5).map(|i| edge(i, (i + 1) % 5)).collect(),
        );
        let star = MolGraph::new(vec![atom(6); 5], (1..5).map(|i| edge(0, i)).collect());
        let ring_gens = vec![vec![1, 2, 3, 4, 0], vec![0, 4, 3, 2, 1]];
        let star_gens = vec![vec![0, 2, 3, 4, 1], vec![0, 2, 1, 3, 4]];
        let mut removed = 0;
        for bits in (0..1024).step_by(17) {
            let mut q = graph(5, bits);
            q.atoms[1].atomic_number = 8;
            for (r, gens) in [(&ring, &ring_gens), (&star, &star_gens)] {
                assert_solution(r, &q, gens);
                removed += cost_mapping_with_generators(r, &q, gens)
                    .unwrap()
                    .1
                    .symmetry_alternatives_removed;
            }
        }
        assert!(removed > 0);
    }
    #[test]
    fn negative_bridge_is_not_pruned() {
        let r = MolGraph::new(vec![atom(6); 5], (0..4).map(|i| edge(i, i + 1)).collect());
        let mut q = r.clone();
        q.atoms[2].atomic_number = 7;
        assert_solution(&r, &q, &[]);
        let result = cost_mapping(&r, &q).unwrap();
        assert_eq!(result.cost, 11);
        assert_eq!(result.map.len(), 5);
    }
    #[test]
    fn assignment_matches_exhaustive_assignments() {
        fn brute_w(w: &[Vec<i64>], i: usize, used: &mut [bool]) -> i64 {
            if i == w.len() {
                return 0;
            }
            let mut best = brute_w(w, i + 1, used);
            for j in 0..used.len() {
                if !used[j] {
                    used[j] = true;
                    best = best.max(w[i][j] + brute_w(w, i + 1, used));
                    used[j] = false;
                }
            }
            best
        }
        for k in 0..100 {
            let w: Vec<Vec<_>> = (0..5)
                .map(|i| {
                    (0..4)
                        .map(|j| ((k + 3 * i + 7 * j + i * j) % 19) as i64)
                        .collect()
                })
                .collect();
            assert_eq!(assignment_upper(&w), brute_w(&w, 0, &mut [false; 4]));
        }
    }
    #[test]
    fn invalid_generators_rejected() {
        let r = graph(4, 7);
        assert!(cost_mapping_with_generators(&r, &r, &[vec![0, 0, 2, 3]]).is_err());
        assert!(cost_mapping_with_generators(&r, &r, &[vec![0, 1]]).is_err());
        let mut r = graph(2, 1);
        r.atoms[1].atomic_number = 8;
        assert!(cost_mapping_with_generators(&r, &r, &[vec![1, 0]]).is_err());
    }
    #[test]
    fn empty_and_single_atom_inputs() {
        assert!(cost_mapping(&graph(0, 0), &graph(1, 0)).is_none());
        let r = MolGraph::new(vec![atom(8)], vec![]);
        let q = MolGraph::new(vec![atom(6)], vec![]);
        // Nonempty is part of the contract, even if the empty map would cost 5.
        assert_eq!(cost_mapping(&r, &q).unwrap().cost, 11);
    }
    #[test]
    fn medium_related_molecule_smoke_benchmark() {
        let mut r = MolGraph::new(
            vec![atom(6); 12],
            (0..12).map(|i| edge(i, (i + 1) % 12)).collect(),
        );
        for i in 0..12 {
            let mut last = i;
            for j in 0..i % 4 {
                let a = r.n_atoms();
                r.atoms.push(atom(if j == 1 { 8 } else { 6 }));
                r.bonds.push(edge(last, a));
                last = a;
            }
        }
        let mut q = r.clone();
        q.atoms[8].atomic_number = 7;
        let start = std::time::Instant::now();
        let (result, stats) = cost_mapping_with_generators(&r, &q, &[]).unwrap();
        let result = result.unwrap();
        assert_eq!(result.cost, mapping_cost(&q, &r, &result.map));
        eprintln!(
            "synthetic {}-atom related pair: {:?}; {stats:?}; cost={}",
            q.n_atoms(),
            start.elapsed(),
            result.cost
        );
    }

    #[test]
    fn greedy_only_maps_hexane_onto_cyclohexane() {
        let hexane = MolGraph::new(vec![atom(6); 6], (0..5).map(|i| edge(i, i + 1)).collect());
        let mut bonds: Vec<_> = (0..5).map(|i| edge(i, i + 1)).collect();
        bonds.push(edge(0, 5));
        let cyclo = MolGraph::new(vec![atom(6); 6], bonds);
        let limits = SearchLimits {
            greedy_only: true,
            ..SearchLimits::unbounded()
        };
        let (res, stats) =
            cost_mapping_with_generators_limited(&hexane, &cyclo, &[], &limits).unwrap();
        let res = res.expect("greedy map");
        assert_eq!(res.map.len(), 6, "map={:?}", res.map);
        assert!(!stats.timed_out);
        assert_eq!(res.cost, mapping_cost(&cyclo, &hexane, &res.map));
    }

    fn oxolane() -> MolGraph {
        MolGraph::new(
            vec![atom(8), atom(6), atom(6), atom(6), atom(6)],
            vec![edge(0, 1), edge(1, 2), edge(2, 3), edge(3, 4), edge(4, 0)],
        )
    }

    fn indane() -> MolGraph {
        // Benzene 0-5 fused to a 5-ring 0-5-6-7-8.
        let mut bonds: Vec<_> = (0..5).map(|i| edge(i, i + 1)).collect();
        bonds.push(edge(5, 0));
        bonds.push(edge(5, 6));
        bonds.push(edge(6, 7));
        bonds.push(edge(7, 8));
        bonds.push(edge(8, 0));
        MolGraph::new(vec![atom(6); 9], bonds)
    }

    #[test]
    fn ring_seed_maps_oxolane_oxygen_to_oxygen() {
        let r = oxolane();
        let q = oxolane();
        let res = cost_mapping(&r, &q).expect("map");
        assert_eq!(res.map.len(), 5, "map={:?}", res.map);
        assert_eq!(
            res.map.get(&0),
            Some(&0),
            "O should map to O: {:?}",
            res.map
        );
        assert_eq!(res.cost, 0);
    }

    #[test]
    fn house_timeout_locks_unique_five_ring_on_indane() {
        let r = indane();
        let mut q = indane();
        let extra = q.n_atoms();
        q.atoms.push(atom(6));
        q.bonds.push(edge(6, extra));
        let limits = SearchLimits::house();
        let (res, _stats) = cost_mapping_with_generators_limited(&r, &q, &[], &limits).unwrap();
        let res = res.expect("map");
        let five_q: HashSet<u16> = [0u16, 5, 6, 7, 8].into_iter().collect();
        let five_r: HashSet<u16> = [0u16, 5, 6, 7, 8].into_iter().collect();
        let mapped_five: Vec<_> = res.map.iter().filter(|(a, _)| five_q.contains(a)).collect();
        assert_eq!(mapped_five.len(), 5, "map={:?}", res.map);
        assert!(
            mapped_five.iter().all(|(_, b)| five_r.contains(b)),
            "5-ring mapped off the 5-ring: {:?}",
            res.map
        );
        assert!(!res.map.contains_key(&(extra as u16)));
    }

    #[test]
    fn different_size_rings_still_map_as_path() {
        let twelve = MolGraph::new(
            vec![atom(6); 12],
            (0..12).map(|i| edge(i, (i + 1) % 12)).collect(),
        );
        let fifteen = MolGraph::new(
            vec![atom(6); 15],
            (0..15).map(|i| edge(i, (i + 1) % 15)).collect(),
        );
        let res = cost_mapping(&twelve, &fifteen).expect("map");
        assert_eq!(res.map.len(), 12, "map={:?}", res.map);
    }

    #[test]
    fn max_states_cap_returns_incumbent_without_hanging() {
        let mut r = MolGraph::new(
            vec![atom(6); 12],
            (0..12).map(|i| edge(i, (i + 1) % 12)).collect(),
        );
        for i in 0..12 {
            let mut last = i;
            for j in 0..i % 4 {
                let a = r.n_atoms();
                r.atoms.push(atom(if j == 1 { 8 } else { 6 }));
                r.bonds.push(edge(last, a));
                last = a;
            }
        }
        let mut q = r.clone();
        q.atoms[8].atomic_number = 7;
        let limits = SearchLimits {
            max_states: Some(8),
            ..SearchLimits::unbounded()
        };
        let start = Instant::now();
        let (res, stats) = cost_mapping_with_generators_limited(&r, &q, &[], &limits).unwrap();
        assert!(stats.timed_out, "cap should fire; {stats:?}");
        assert!(res.is_some(), "incumbent should survive a cap");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "capped search hung: {:?}",
            start.elapsed()
        );
    }
}

#[cfg(test)]
mod canonaut_tests {
    use super::*;

    fn atom(z: u8) -> AtomFeat {
        AtomFeat {
            atomic_number: z,
            formal_charge: 0,
            radical: 0,
            chirality: AtomChirality::None,
        }
    }
    fn bond(a: u16, b: u16, order: u8) -> BondFeat {
        BondFeat {
            begin: a,
            end: b,
            order,
            aromatic: false,
            stereo: EzStereo::None,
            stereo_atom1: None,
            stereo_atom2: None,
        }
    }
    fn chain_ring(chain: u16, ring: u16) -> MolGraph {
        let n = chain + ring;
        let mut bonds: Vec<_> = (1..n).map(|i| bond(i - 1, i, 1)).collect();
        if ring > 2 {
            bonds.push(bond(chain, n - 1, 1));
        }
        MolGraph::new(vec![atom(6); n as usize], bonds)
    }

    #[test]
    fn cyclohexane_generators_act_on_ring() {
        let ring = MolGraph::new(
            vec![atom(6); 6],
            (0..6).map(|i| bond(i, (i + 1) % 6, 1)).collect(),
        );
        let gens = reference_generators(&ring);
        assert!(!gens.is_empty());
        let (res, stats) = cost_mapping_with_generators(&ring, &ring, &gens).unwrap();
        assert_eq!(res.unwrap().cost, 0);
        // One reference orbit: a perfect map is found from the first seed.
        assert_eq!(stats.seeds, 1);
    }

    #[test]
    fn generators_respect_bond_order() {
        // Kekulé-free 1,3-butadiene: ends swap, but C1–C2 never maps to C2–C3.
        let diene = MolGraph::new(
            vec![atom(6); 4],
            vec![bond(0, 1, 2), bond(1, 2, 1), bond(2, 3, 2)],
        );
        for g in reference_generators(&diene) {
            assert!(g == [3, 2, 1, 0] || g == [0, 1, 2, 3], "{g:?}");
        }
    }

    #[test]
    fn optimal_assignment_maximizes_gain_and_drops_losers() {
        // 2×3: greedy would take (0,0)=9 then (1,1)=1; optimum is 8+7.
        let gain = [9, 8, 0, 7, 1, -4];
        assert_eq!(optimal_assignment(&gain, 2, 3), vec![Some(1), Some(0)]);
        // Only negative gains: nothing assigned.
        assert_eq!(optimal_assignment(&[-1, -2], 1, 2), vec![None]);
    }

    #[test]
    fn symmetry_does_not_change_optimum() {
        let pentyl = chain_ring(5, 6);
        let hexane = chain_ring(6, 0);
        for (r, q) in [(&pentyl, &hexane), (&hexane, &pentyl), (&pentyl, &pentyl)] {
            let plain = cost_mapping_with_generators(r, q, &[]).unwrap().0.unwrap();
            let sym = cost_mapping(r, q).unwrap();
            assert_eq!(plain.cost, sym.cost);
            assert_eq!(mapping_cost(q, r, &sym.map), sym.cost);
        }
    }
}

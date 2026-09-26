//! House MCS via **chematic** (element + hybridization, any-bond).
//!
//! Clients call [`mcs_atom_map`] / [`mcs_atom_map_graph`] instead of
//! implementing RDKit FMCS / MinimalLib MCS themselves. Depictor / MinimalLib
//! still own **2D coordinates** — chematic is not used for layout.
//!
//! Protocol (parity with prior Python/JS/native hosts):
//! 1. Tag copies with isotopes ``Z×10+hyb`` (SP2=2, SP3=3).
//! 2. [`chematic::smarts::find_mcs`] with `AtomCompare::Elements`,
//!    `BondCompare::Any`, `match_isotope`, `match_bonds: false` (so the
//!    emitted query uses `BondPrimitive::Any` and rematches aromatic ↔
//!    kekulé targets).
//! 3. [`chematic::smarts::find_matches`] on both tagged mols →
//!    `(query_idx, template_idx)` pairs.

use chematic::core::{Atom, AtomIdx, BondOrder, Element, Molecule, MoleculeBuilder};
use chematic::smarts::{
    find_matches, find_mcs_with_config, AtomCompare, BondCompare, McsConfig,
};
use chematic::smiles;
use serde::{Deserialize, Serialize};

use crate::cxsmiles::smiles_base;
use crate::edge::MIN_MCS_ATOMS;

/// Default MCS timeout (ms) — parity with prior host `Timeout: 2`.
pub const MCS_TIMEOUT_MS: u64 = 2000;

/// Wire graph for MCS when SMILES atom order is unavailable (layouts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McsMolIn {
    pub atoms: Vec<McsAtomIn>,
    pub bonds: Vec<McsBondIn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McsAtomIn {
    /// Atomic number (`0` = wildcard `*`).
    pub z: u32,
    /// Optional; inferred from aromatic bonds when omitted.
    #[serde(default)]
    pub aromatic: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McsBondIn {
    pub begin: u32,
    pub end: u32,
    /// 1 / 1.5 / 2 / 3 (1.5 → aromatic).
    pub order: f64,
}

fn bond_weight(order: BondOrder) -> f64 {
    match order {
        BondOrder::Single | BondOrder::Up | BondOrder::Down => 1.0,
        BondOrder::Double => 2.0,
        BondOrder::Triple => 3.0,
        BondOrder::Aromatic => 1.5,
        _ => 1.0,
    }
}

/// SP2=2 vs SP3=3 — parity with JS `inferHybridizationCodes`.
fn hyb_code(mol: &Molecule, idx: u32) -> u16 {
    let atom = mol.atom(AtomIdx(idx));
    if atom.aromatic {
        return 2;
    }
    let mut max_bo = 1.0_f64;
    for (_, bond_idx) in mol.neighbors(AtomIdx(idx)) {
        max_bo = max_bo.max(bond_weight(mol.bond(bond_idx).order));
    }
    if max_bo >= 1.5 {
        return 2;
    }
    let mut adj_unsat = false;
    for (nb, _) in mol.neighbors(AtomIdx(idx)) {
        for (_, b2) in mol.neighbors(nb) {
            if bond_weight(mol.bond(b2).order) >= 1.5 {
                adj_unsat = true;
                break;
            }
        }
        if adj_unsat {
            break;
        }
    }
    let z = atom.element.atomic_number();
    if z != 6 && adj_unsat {
        2
    } else {
        3
    }
}

fn tag_hybridization_isotopes(mol: &Molecule) -> Molecule {
    let mut out = mol.clone();
    for i in 0..mol.atom_count() {
        let z = u16::from(mol.atom(AtomIdx(i as u32)).element.atomic_number());
        let iso = z.saturating_mul(10).saturating_add(hyb_code(mol, i as u32));
        out.set_isotope(AtomIdx(i as u32), Some(iso));
    }
    out
}

fn house_mcs_config(min_atoms: u32) -> McsConfig {
    McsConfig {
        atom_compare: AtomCompare::Elements,
        bond_compare: BondCompare::Any,
        // Emit BondPrimitive::Any so aromatic (phenol) MCS rematches kekulé
        // (quinone) — BondCompare::Any only applies during search growth.
        match_bonds: false,
        match_isotope: true,
        min_atoms: min_atoms as usize,
        timeout_ms: Some(MCS_TIMEOUT_MS),
        ..McsConfig::default()
    }
}

fn mcs_pair_candidates(
    query: &Molecule,
    template: &Molecule,
    min_atoms: u32,
) -> Vec<Vec<(u32, u32)>> {
    let qtag = tag_hybridization_isotopes(query);
    let ttag = tag_hybridization_isotopes(template);
    let cfg = house_mcs_config(min_atoms);
    let pattern = find_mcs_with_config(&[&qtag, &ttag], &cfg);
    if pattern.atom_count() < min_atoms as usize {
        return Vec::new();
    }
    let qm = find_matches(&pattern, &qtag);
    let tm = find_matches(&pattern, &ttag);
    if qm.is_empty() || tm.is_empty() {
        return Vec::new();
    }
    // Cap orientations so choose_mapping stays bounded (parity with old
    // Python place/order caps).
    const PLACE_CAP: usize = 8;
    const ORDER_CAP: usize = 24;
    let mut out = Vec::new();
    for qmatch in qm.iter().take(PLACE_CAP * ORDER_CAP) {
        for tmatch in tm.iter().take(ORDER_CAP) {
            let mut pairs = Vec::with_capacity(qmatch.len());
            for (qi, qa) in qmatch {
                if let Some(ta) = tmatch.get(qi) {
                    pairs.push((qa.0, ta.0));
                }
            }
            if pairs.len() >= min_atoms as usize {
                out.push(pairs);
            }
            if out.len() >= PLACE_CAP * ORDER_CAP {
                return out;
            }
        }
    }
    out
}

fn mcs_pairs(query: &Molecule, template: &Molecule, min_atoms: u32) -> Option<Vec<(u32, u32)>> {
    mcs_pair_candidates(query, template, min_atoms).into_iter().next()
}

fn parse_smiles_mol(source: &str) -> Option<Molecule> {
    let base = smiles_base(source);
    if base.trim().is_empty() {
        return None;
    }
    smiles::parse(&base).ok()
}

fn order_to_bond(order: f64) -> BondOrder {
    if (order - 1.5).abs() < 0.1 {
        BondOrder::Aromatic
    } else if order >= 2.5 {
        BondOrder::Triple
    } else if order >= 1.5 {
        BondOrder::Double
    } else {
        BondOrder::Single
    }
}

fn graph_to_mol(g: &McsMolIn) -> Option<Molecule> {
    if g.atoms.is_empty() {
        return None;
    }
    let mut b = MoleculeBuilder::with_capacity(g.atoms.len(), g.bonds.len());
    for atom in &g.atoms {
        let mut a = if atom.z == 0 {
            Atom::wildcard()
        } else {
            let elem = Element::from_atomic_number(atom.z as u8)?;
            Atom::new(elem)
        };
        if atom.aromatic == Some(true) {
            a.aromatic = true;
        }
        b.add_atom(a);
    }
    for bond in &g.bonds {
        b.add_bond(AtomIdx(bond.begin), AtomIdx(bond.end), order_to_bond(bond.order))
            .ok()?;
    }
    let mut mol = b.build();
    for bond in &g.bonds {
        if order_to_bond(bond.order) == BondOrder::Aromatic {
            mol.set_atom_aromatic(AtomIdx(bond.begin), true);
            mol.set_atom_aromatic(AtomIdx(bond.end), true);
        }
    }
    for (i, atom) in g.atoms.iter().enumerate() {
        if atom.aromatic == Some(true) {
            mol.set_atom_aromatic(AtomIdx(i as u32), true);
        }
    }
    Some(mol)
}

/// House MCS atom map from SMILES / CXSMILES sources.
///
/// Returns `(query_atom, template_atom)` pairs, or ``None`` when the MCS is
/// below `min_atoms` (default [`MIN_MCS_ATOMS`]).
pub fn mcs_atom_map(
    query_smiles: &str,
    template_smiles: &str,
    min_atoms: Option<u32>,
) -> Option<Vec<(u32, u32)>> {
    let floor = min_atoms.unwrap_or(MIN_MCS_ATOMS);
    let q = parse_smiles_mol(query_smiles)?;
    let t = parse_smiles_mol(template_smiles)?;
    mcs_pairs(&q, &t, floor)
}

/// House MCS from explicit atom/bond graphs (layout indices).
pub fn mcs_atom_map_graph(
    query: &McsMolIn,
    template: &McsMolIn,
    min_atoms: Option<u32>,
) -> Option<Vec<(u32, u32)>> {
    let floor = min_atoms.unwrap_or(MIN_MCS_ATOMS);
    let q = graph_to_mol(query)?;
    let t = graph_to_mol(template)?;
    mcs_pairs(&q, &t, floor)
}

/// All MCS embeddings (query×template match orientations) for rigid ranking.
pub fn mcs_atom_map_graph_candidates(
    query: &McsMolIn,
    template: &McsMolIn,
    min_atoms: Option<u32>,
) -> Vec<Vec<(u32, u32)>> {
    let floor = min_atoms.unwrap_or(MIN_MCS_ATOMS);
    let Some(q) = graph_to_mol(query) else {
        return Vec::new();
    };
    let Some(t) = graph_to_mol(template) else {
        return Vec::new();
    };
    mcs_pair_candidates(&q, &t, floor)
}

/// JSON helpers for language bindings (`[[q,t], …]` or `null`).
pub fn mcs_atom_map_json(
    query_smiles: &str,
    template_smiles: &str,
    min_atoms: Option<u32>,
) -> String {
    match mcs_atom_map(query_smiles, template_smiles, min_atoms) {
        Some(pairs) => serde_json::to_string(&pairs).unwrap_or_else(|_| "null".into()),
        None => "null".into(),
    }
}

pub fn mcs_atom_map_graph_json(
    query_json: &str,
    template_json: &str,
    min_atoms: Option<u32>,
) -> Result<String, String> {
    let q: McsMolIn =
        serde_json::from_str(query_json).map_err(|e| format!("query McsMolIn JSON: {e}"))?;
    let t: McsMolIn =
        serde_json::from_str(template_json).map_err(|e| format!("template McsMolIn JSON: {e}"))?;
    Ok(match mcs_atom_map_graph(&q, &t, min_atoms) {
        Some(pairs) => serde_json::to_string(&pairs).unwrap_or_else(|_| "null".into()),
        None => "null".into(),
    })
}

pub fn mcs_atom_map_graph_candidates_json(
    query_json: &str,
    template_json: &str,
    min_atoms: Option<u32>,
) -> Result<String, String> {
    let q: McsMolIn =
        serde_json::from_str(query_json).map_err(|e| format!("query McsMolIn JSON: {e}"))?;
    let t: McsMolIn =
        serde_json::from_str(template_json).map_err(|e| format!("template McsMolIn JSON: {e}"))?;
    let maps = mcs_atom_map_graph_candidates(&q, &t, min_atoms);
    serde_json::to_string(&maps).map_err(|e| format!("candidates JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phenol_quinone_maps() {
        let m = mcs_atom_map("CCc1ccccc1O", "O=C1C=CC(=O)C=C1", None).unwrap();
        assert!(m.len() >= 6, "got {}", m.len());
    }

    #[test]
    fn quinone_phenol_maps() {
        let m = mcs_atom_map("O=C1C=CC(=O)C=C1", "CCc1ccccc1O", None).unwrap();
        assert!(m.len() >= 6);
    }

    #[test]
    fn aliphatic_vs_quinone_rejected() {
        assert!(mcs_atom_map("C1CCCCC1O", "O=C1C=CC(=O)C=C1", None).is_none());
    }

    #[test]
    fn thp_chain_maps() {
        let m = mcs_atom_map("O=CCCCCO", "C1CCCOC1", None).unwrap();
        assert!(m.len() >= 4);
    }

    #[test]
    fn ethyl_pentyl_phenyl_maps() {
        let m = mcs_atom_map("c1ccccc1CC", "c1ccccc1CCCCC", None).unwrap();
        assert!(m.len() >= 8);
    }

    #[test]
    fn graph_benzene_scaffold() {
        let q = McsMolIn {
            atoms: (0..6)
                .map(|_| McsAtomIn {
                    z: 6,
                    aromatic: Some(true),
                })
                .collect(),
            bonds: vec![
                McsBondIn {
                    begin: 0,
                    end: 1,
                    order: 1.5,
                },
                McsBondIn {
                    begin: 1,
                    end: 2,
                    order: 1.5,
                },
                McsBondIn {
                    begin: 2,
                    end: 3,
                    order: 1.5,
                },
                McsBondIn {
                    begin: 3,
                    end: 4,
                    order: 1.5,
                },
                McsBondIn {
                    begin: 4,
                    end: 5,
                    order: 1.5,
                },
                McsBondIn {
                    begin: 5,
                    end: 0,
                    order: 1.5,
                },
            ],
        };
        let t = q.clone();
        let m = mcs_atom_map_graph(&q, &t, None).unwrap();
        assert_eq!(m.len(), 6);
    }

    #[test]
    fn json_null_on_reject() {
        assert_eq!(
            mcs_atom_map_json("C1CCCCC1O", "O=C1C=CC(=O)C=C1", None),
            "null"
        );
    }
}

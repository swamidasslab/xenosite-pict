//! Structure parse: SMILES / CXSMILES / molfile → CoordGen graph + H / radicals.
//!
//! Aromaticity is taken from the input (lowercase SMILES, CTAB bond type 4).
//! No Chematic; no re-perception.

use crate::layout_coordgen::LayoutMolecule;
use crate::molfile::parse_molfile;
use crate::smiles::parse_smiles;
use crate::valence::{implicit_hcount, unpaired_electrons};

#[derive(Clone, Debug)]
pub struct AtomRec {
    pub z: i32,
    pub charge: i32,
    pub aromatic: bool,
    /// `None` = organic-subset (infer H). `Some` = bracket / explicit CTAB H.
    pub hcount: Option<u8>,
    /// CTAB `M  RAD` / charge-code 4. SMILES radicals use H deficit instead.
    pub radical: u8,
    pub mapno: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BondKind {
    Single,
    Double,
    Triple,
    Quadruple,
    Aromatic,
    /// `a` is the donor, `b` the acceptor.
    Dative,
}

#[derive(Clone, Debug)]
pub struct BondRec {
    pub a: usize,
    pub b: usize,
    pub kind: BondKind,
}

#[derive(Clone, Debug)]
pub struct MolGraph {
    pub atoms: Vec<AtomRec>,
    pub bonds: Vec<BondRec>,
}

#[derive(Clone, Debug)]
pub struct Parsed {
    pub layout: LayoutMolecule,
    pub atom_indices: Vec<i32>,
    pub paint_orders: Vec<f64>,
    pub implicit_hs: Vec<u32>,
    pub radicals: Vec<u8>,
    pub atom_mapnos: Vec<Option<u32>>,
}

pub fn parse_source(source: &str) -> Result<Parsed, String> {
    let text = source.trim();
    if text.is_empty() {
        return Err("empty structure".into());
    }
    let g = if text.contains("V2000") || text.contains("V3000") || text.lines().count() >= 4 {
        parse_molfile(text)?
    } else {
        parse_smiles(text).map_err(|e| format!("smiles: {e}"))?
    };
    graph_to_parsed(&g)
}

pub fn graph_to_parsed(g: &MolGraph) -> Result<Parsed, String> {
    let n = g.atoms.len();
    if n == 0 {
        return Err("empty molecule".into());
    }
    let mut atomic_numbers = Vec::with_capacity(n);
    let mut formal_charges = Vec::with_capacity(n);
    let mut implicit_hs = Vec::with_capacity(n);
    let mut radicals = Vec::with_capacity(n);
    let mut atom_mapnos = Vec::with_capacity(n);
    for i in 0..n {
        atomic_numbers.push(g.atoms[i].z);
        formal_charges.push(g.atoms[i].charge);
        implicit_hs.push(u32::from(implicit_hcount(&g.atoms, &g.bonds, i)));
        radicals.push(unpaired_electrons(&g.atoms, &g.bonds, i).max(g.atoms[i].radical));
        atom_mapnos.push(g.atoms[i].mapno);
    }
    let mut bond_from = Vec::new();
    let mut bond_to = Vec::new();
    let mut bond_orders = Vec::new();
    let mut paint_orders = Vec::new();
    for b in &g.bonds {
        let order = ctab_bond_order(
            b.kind,
            g.atoms.get(b.a).is_some_and(|a| a.aromatic),
            g.atoms.get(b.b).is_some_and(|a| a.aromatic),
        );
        bond_from.push(b.a as i32);
        bond_to.push(b.b as i32);
        bond_orders.push(order);
        paint_orders.push(order as f64);
    }
    let nb = bond_from.len();
    Ok(Parsed {
        layout: LayoutMolecule {
            atomic_numbers,
            formal_charges,
            bond_from,
            bond_to,
            bond_orders,
            bond_stereos: vec![0; nb],
            ..Default::default()
        },
        atom_indices: (0..n as i32).collect(),
        paint_orders,
        implicit_hs,
        radicals,
        atom_mapnos,
    })
}

fn ctab_bond_order(kind: BondKind, a_arom: bool, b_arom: bool) -> i32 {
    match kind {
        BondKind::Aromatic | BondKind::Quadruple => 4,
        BondKind::Double => 2,
        BondKind::Triple => 3,
        BondKind::Single | BondKind::Dative if a_arom && b_arom => 4,
        _ => 1,
    }
}

//! Experimental 2D layout via **chematic** (`compute_layout` / `compute_depict_data`).
//!
//! Host-only: gated by Cargo feature ``chematic-layout`` and a Python/JS
//! runtime flag — **not** part of DepictSpec / EdgePlan schema. Default
//! production coords remain RDKit Depictor / MinimalLib.

use chematic::core::{BondOrder, Element};
use chematic::depict::{compute_depict_data, DepictBondKind, BOND_LEN};
use chematic::smiles;

use crate::cxsmiles::smiles_base;
use crate::metrics::SCALE;
use crate::scene::{AtomIn, BondIn, MoleculeIn};

fn element_label(el: &str, hcount: u8, charge: i32, isotope: Option<u16>) -> Option<String> {
    if el == "C" && charge == 0 && isotope.is_none() && hcount <= 4 {
        return None;
    }
    if el == "*" || el.starts_with('R') || el.starts_with('_') {
        return Some("*".to_string());
    }
    let mut text = if hcount == 0 {
        el.to_string()
    } else if hcount == 1 {
        format!("{el}H")
    } else {
        format!("{el}H{hcount}")
    };
    if let Some(mass) = isotope {
        text = format!("{mass}{text}");
    }
    if charge != 0 {
        let sign = if charge > 0 { "+" } else { "−" };
        let mag = charge.unsigned_abs();
        if mag == 1 {
            text.push_str(sign);
        } else {
            text.push_str(&format!("{mag}{sign}"));
        }
    }
    Some(text)
}

fn bond_order(kind: &DepictBondKind) -> f64 {
    match kind {
        DepictBondKind::Single | DepictBondKind::Up | DepictBondKind::Down => 1.0,
        DepictBondKind::Double => 2.0,
        DepictBondKind::Triple => 3.0,
        DepictBondKind::Aromatic => 1.5,
    }
}

fn bond_stereo(kind: &DepictBondKind) -> Option<&'static str> {
    match kind {
        DepictBondKind::Up => Some("up"),
        DepictBondKind::Down => Some("down"),
        _ => None,
    }
}

fn element_symbol(el: Element) -> String {
    if el.atomic_number() == 0 {
        "*".to_string()
    } else {
        el.symbol().to_string()
    }
}

/// Chematic 2D layout → house [`MoleculeIn`] (coords scaled to [`SCALE`]).
///
/// Chematic places atoms in SVG Y-down space at ``BOND_LEN`` (~40). We scale
/// by ``SCALE / BOND_LEN`` so mean bond length matches the RDKit host path.
pub fn layout_with_chematic(source: &str, id: Option<String>) -> Result<MoleculeIn, String> {
    let base = smiles_base(source);
    if base.trim().is_empty() {
        return Err("empty SMILES for chematic layout".into());
    }
    let mol = smiles::parse(&base).map_err(|e| format!("chematic SMILES parse: {e}"))?;
    let data = compute_depict_data(&mol);
    let scale = SCALE / BOND_LEN;

    let atoms: Vec<AtomIn> = data
        .atoms
        .iter()
        .map(|a| {
            let idx = a.idx.0 as i32;
            let z = u32::from(a.element.atomic_number());
            let el = element_symbol(a.element);
            let charge = i32::from(a.charge);
            let hcount = mol.implicit_hydrogen_count(a.idx);
            let isotope = mol.atom(a.idx).isotope;
            let label = element_label(&el, hcount, charge, isotope);
            AtomIn {
                index: idx,
                element: Some(el),
                z: Some(z),
                x: a.pos.x * scale,
                y: a.pos.y * scale,
                label,
                charge,
            }
        })
        .collect();

    let bonds: Vec<BondIn> = data
        .bonds
        .iter()
        .map(|b| {
            let mut bond = BondIn {
                index: b.idx.0 as i32,
                begin: b.atom1.0 as i32,
                end: b.atom2.0 as i32,
                order: bond_order(&b.kind),
                stereo: bond_stereo(&b.kind).map(str::to_string),
                interior: None,
            };
            // Prefer explicit bond order from the molecule when depict kind
            // collapsed Up/Down onto singles.
            if matches!(b.kind, DepictBondKind::Up | DepictBondKind::Down) {
                match mol.bond(b.idx).order {
                    BondOrder::Double => bond.order = 2.0,
                    BondOrder::Triple => bond.order = 3.0,
                    BondOrder::Aromatic => bond.order = 1.5,
                    _ => {}
                }
            }
            bond
        })
        .collect();

    Ok(MoleculeIn {
        id,
        atoms,
        bonds,
        color: None,
        atom_shade: None,
        bond_shade: None,
        shade_vmin: None,
        shade_vmax: None,
        mark_atoms: Vec::new(),
        mark_bonds: Vec::new(),
        scale: 1.0,
        weight: 1.0,
    })
}

/// JSON ABI for hosts: ``MoleculeIn`` or error string.
pub fn layout_with_chematic_json(source: &str, id: Option<&str>) -> Result<String, String> {
    let mol = layout_with_chematic(source, id.map(str::to_string))?;
    serde_json::to_string(&mol).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspirin_has_atoms_and_bonds() {
        let mol = layout_with_chematic("CC(=O)Oc1ccccc1C(=O)O", Some("asp".into())).unwrap();
        assert_eq!(mol.id.as_deref(), Some("asp"));
        assert!(mol.atoms.len() >= 10);
        assert!(!mol.bonds.is_empty());
        let mean = {
            let mut lens = Vec::new();
            for b in &mol.bonds {
                let a = mol.atoms.iter().find(|x| x.index == b.begin).unwrap();
                let c = mol.atoms.iter().find(|x| x.index == b.end).unwrap();
                lens.push((c.x - a.x).hypot(c.y - a.y));
            }
            lens.iter().sum::<f64>() / lens.len() as f64
        };
        assert!((mean - SCALE).abs() < 2.0, "mean bond {mean} vs SCALE {SCALE}");
    }
}

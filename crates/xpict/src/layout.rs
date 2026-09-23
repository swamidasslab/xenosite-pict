//! RDKit layout via crates.io `rdkit` + local Depictor FFI.

use rdkit::{ROMol, RWMol};
use xpict_core::scene::{AtomIn, BondIn, MoleculeIn};
use xpict_core::SCALE;

use crate::cxsmiles::smiles_base;
use crate::ffi::ffi::{self, LayoutOut};
use crate::Error;

/// Mean bond length in RDKit units → scale factor so mean ≈ [`SCALE`].
fn mean_bond_scale(atoms: &[(f64, f64)], bonds: &[(i32, i32)]) -> f64 {
    let mut lengths = Vec::new();
    for &(i, j) in bonds {
        let (Some(a), Some(b)) = (
            atoms.get(i as usize),
            atoms.get(j as usize),
        ) else {
            continue;
        };
        lengths.push((b.0 - a.0).hypot(b.1 - a.1));
    }
    let mean = if lengths.is_empty() {
        1.5
    } else {
        lengths.iter().sum::<f64>() / lengths.len() as f64
    };
    let mean = if mean < 1e-6 { 1.5 } else { mean };
    SCALE / mean
}

fn atom_label(element: &str, imp_hs: u32, charge: i32) -> Option<String> {
    if element == "C" && charge == 0 {
        return None;
    }
    if element == "*" {
        return Some("*".into());
    }
    let mut text = element.to_string();
    if imp_hs == 1 {
        text = format!("{element}H");
    } else if imp_hs > 1 {
        text = format!("{element}H{imp_hs}");
    }
    if charge != 0 {
        let sign = if charge > 0 { "+" } else { "−" };
        let mag = charge.unsigned_abs();
        text = if mag == 1 {
            format!("{text}{sign}")
        } else {
            format!("{text}{mag}{sign}")
        };
    }
    Some(text)
}

fn looks_like_molblock(source: &str) -> bool {
    source.contains("V2000") || source.contains("V3000")
}

/// Rewrite dummy ``R`` / drop ``M  ALS`` so multi-star frames keep stable indices
/// (parity with JS ``sanitizeDummyMolblock``).
pub fn sanitize_dummy_molblock(molblock: &str) -> String {
    molblock
        .lines()
        .filter(|line| !line.starts_with("M  ALS"))
        .map(|line| line.replace(" R ", " * "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse SMILES/molfile with the crates.io ``rdkit`` package → molblock.
pub fn source_to_molblock(source: &str) -> Result<String, Error> {
    let text = source.trim();
    if text.is_empty() {
        return Err(Error::EmptySource);
    }
    if looks_like_molblock(text) {
        let rw = RWMol::from_mol_block(text, true, false, true)
            .ok_or_else(|| Error::Parse(format!("RDKit could not parse molblock ({})", &text[..text.len().min(80)])))?;
        Ok(sanitize_dummy_molblock(&rw.to_ro_mol().to_molblock()))
    } else {
        let smiles = smiles_base(text);
        let mol = ROMol::from_smiles(&smiles).map_err(|e| Error::Parse(e.to_string()))?;
        Ok(sanitize_dummy_molblock(&mol.to_molblock()))
    }
}

fn layout_out_to_molecule(
    laid: &LayoutOut,
    id: Option<String>,
    scale: f64,
    flip_max_y: f64,
) -> MoleculeIn {
    let atoms: Vec<AtomIn> = laid
        .atoms
        .iter()
        .map(|a| {
            let element = if a.z == 0 {
                "*".to_string()
            } else if a.symbol.is_empty() {
                xpict_core::element_symbol(a.z as u32).to_string()
            } else {
                a.symbol.clone()
            };
            let label = atom_label(&element, a.total_hs, a.charge);
            AtomIn {
                index: a.index,
                element: Some(element),
                z: Some(a.z.max(0) as u32),
                x: a.x * scale,
                y: (flip_max_y - a.y) * scale,
                label,
                charge: a.charge,
            }
        })
        .collect();

    let bonds: Vec<BondIn> = laid
        .bonds
        .iter()
        .map(|b| BondIn {
            index: b.index,
            begin: b.begin,
            end: b.end,
            order: b.order,
            stereo: if b.stereo.is_empty() {
                None
            } else {
                Some(b.stereo.clone())
            },
            interior: None,
        })
        .collect();

    MoleculeIn {
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
        weight: 1.0,
        scale: 1.0,
    }
}

/// Layout ``source`` (SMILES / CXSMILES / molfile) with optional template align.
///
/// Returns ``(MoleculeIn in SCALE space, pose molblock)``.
pub fn layout_with_rdkit(
    source: &str,
    template_molblock: Option<&str>,
    id: Option<String>,
) -> Result<(MoleculeIn, String), Error> {
    let mb = source_to_molblock(source)?;
    let tmpl = template_molblock.unwrap_or("").to_string();
    let tmpl = if tmpl.is_empty() {
        String::new()
    } else {
        sanitize_dummy_molblock(&tmpl)
    };

    let laid = ffi::prepare_layout(&mb, &tmpl).map_err(|e| Error::Layout(e.to_string()))?;

    let coords: Vec<(f64, f64)> = laid.atoms.iter().map(|a| (a.x, a.y)).collect();
    let bond_pairs: Vec<(i32, i32)> = laid.bonds.iter().map(|b| (b.begin, b.end)).collect();

    let (scale, flip_max_y) = if !tmpl.is_empty() {
        // Match template frame scale / flip (JS layoutWithRdkit).
        let tmpl_laid =
            ffi::prepare_layout(&tmpl, "").map_err(|e| Error::Layout(e.to_string()))?;
        let t_coords: Vec<(f64, f64)> = tmpl_laid.atoms.iter().map(|a| (a.x, a.y)).collect();
        let t_bonds: Vec<(i32, i32)> = tmpl_laid
            .bonds
            .iter()
            .map(|b| (b.begin, b.end))
            .collect();
        let scale = mean_bond_scale(&t_coords, &t_bonds);
        let flip_max_y = t_coords
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::NEG_INFINITY, f64::max);
        (scale, if flip_max_y.is_finite() { flip_max_y } else { 0.0 })
    } else {
        let scale = mean_bond_scale(&coords, &bond_pairs);
        let flip_max_y = coords
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::NEG_INFINITY, f64::max);
        (scale, if flip_max_y.is_finite() { flip_max_y } else { 0.0 })
    };

    let molecule = layout_out_to_molecule(&laid, id, scale, flip_max_y);
    let pose = sanitize_dummy_molblock(&laid.molblock);
    Ok((molecule, pose))
}

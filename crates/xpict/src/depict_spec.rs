//! Limited declarative document: list of mols → list of [`Rendered`].
//!
//! This is the expandable stub toward full ``PictSpec``. Only fields that the
//! single-mol client already supports are accepted.

use serde::{Deserialize, Serialize};

use crate::{render, Error, Mol, MolRenderOptions, Rendered};

/// Batch depiction document (MVP).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DepictSpec {
    pub molecules: Vec<MolSpec>,
}

/// One molecule entry — mirrors JS/Python render options + a structure string.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MolSpec {
    /// SMILES or CXSMILES (preferred field name for the document stub).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smiles: Option<String>,
    /// Alias accepted in JSON: ``"source": "CCO"``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub molfile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cxsmiles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atom_shade: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_shade: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_atoms: Option<Vec<i32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_bonds: Option<Vec<(i32, i32)>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub star_labels: Option<Vec<Option<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold_labels: Option<bool>,
    /// Index of an **earlier** molecule in this document to align onto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align_to: Option<usize>,
}

impl MolSpec {
    fn structure(&self) -> Result<&str, Error> {
        if let Some(ref s) = self.molfile {
            if !s.trim().is_empty() {
                return Ok(s.as_str());
            }
        }
        if let Some(ref s) = self.cxsmiles {
            if !s.trim().is_empty() {
                return Ok(s.as_str());
            }
        }
        if let Some(ref s) = self.smiles {
            if !s.trim().is_empty() {
                return Ok(s.as_str());
            }
        }
        if let Some(ref s) = self.source {
            if !s.trim().is_empty() {
                return Ok(s.as_str());
            }
        }
        Err(Error::Message(
            "MolSpec needs smiles, cxsmiles, molfile, or source".into(),
        ))
    }
}

/// Render every molecule in order; ``align_to`` indexes earlier results.
pub fn depict(spec: &DepictSpec) -> Result<Vec<Rendered>, Error> {
    let mut out: Vec<Rendered> = Vec::with_capacity(spec.molecules.len());
    for (i, entry) in spec.molecules.iter().enumerate() {
        let structure = entry.structure()?;
        let mut mol = Mol::from_source(structure)?;
        let align_to = match entry.align_to {
            Some(idx) if idx >= i => {
                return Err(Error::Message(format!(
                    "molecules[{i}].align_to={idx} must refer to an earlier entry"
                )));
            }
            Some(idx) => Some(out[idx].frame_molblock.clone()),
            None => None,
        };
        let opts = MolRenderOptions {
            id: entry.id.clone(),
            color: entry.color.clone(),
            atom_shade: entry.atom_shade.clone(),
            bond_shade: entry.bond_shade.clone(),
            mark_atoms: entry.mark_atoms.clone(),
            mark_bonds: entry.mark_bonds.clone(),
            star_labels: entry.star_labels.clone(),
            bold_labels: entry.bold_labels,
            align_to,
        };
        out.push(render(&mut mol, opts)?);
    }
    Ok(out)
}

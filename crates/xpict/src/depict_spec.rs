//! Declarative document — strict subset of future ``PictSpec``.
//!
//! Root is ``type: "mol"`` or ``type: "group"`` with ``children``. Calls the
//! simple [`crate::render`] / [`crate::mol`] client internally.
//!
//! Markush / star text: [`MolNode::star_labels`] (encounter order) or CXSMILES
//! aliases. Document ``rgroups`` is not on the public document API yet.

use serde::{Deserialize, Serialize};

use crate::{render, Error, Mol, MolRenderOptions, Rendered};

/// Per-atom / per-bond colormap scores (document ``shade``).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShadeSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atoms: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonds: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colormap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmax: Option<f64>,
}

/// Mol node — subset of future ``MolNode``.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MolNode {
    #[serde(rename = "type", default = "mol_type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smiles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cxsmiles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub molfile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shade: Option<ShadeSpec>,
    /// Labels for ``*`` atoms in layout encounter order (chem markup OK).
    /// Wins over CXSMILES aliases when both are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub star_labels: Option<Vec<Option<String>>>,
    /// Uniform diagram scale (``1.0`` = house size). Omitted → ``1.0``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Ink weight relative to house (``1.0``). Min ~``2/3`` (Regular stem).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
}

fn mol_type() -> String {
    "mol".into()
}

/// Declarative document (``mol`` or ``group`` root).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DepictSpec {
    Mol {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        smiles: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cxsmiles: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        molfile: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shade: Option<ShadeSpec>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        star_labels: Option<Vec<Option<String>>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<f64>,
    },
    Group {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        children: Vec<MolNode>,
    },
}

impl DepictSpec {
    fn owned_mols(&self) -> Vec<MolNode> {
        match self {
            DepictSpec::Mol {
                smiles,
                cxsmiles,
                molfile,
                id,
                color,
                shade,
                star_labels,
                scale,
                weight,
            } => vec![MolNode {
                type_: "mol".into(),
                smiles: smiles.clone(),
                cxsmiles: cxsmiles.clone(),
                molfile: molfile.clone(),
                id: id.clone(),
                color: color.clone(),
                shade: shade.clone(),
                star_labels: star_labels.clone(),
                scale: *scale,
                weight: *weight,
            }],
            DepictSpec::Group { children, .. } => children.clone(),
        }
    }
}

impl MolNode {
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
        Err(Error::Message(
            "mol node needs smiles, cxsmiles, or molfile".into(),
        ))
    }
}

/// Alias of [`MolNode`].
pub type MolSpec = MolNode;

/// Render every mol via the simple [`crate::render`] client.
pub fn depict(spec: &DepictSpec) -> Result<Vec<Rendered>, Error> {
    let mut out: Vec<Rendered> = Vec::new();
    for entry in spec.owned_mols() {
        let structure = entry.structure()?;
        let mut mol = Mol::from_source(structure)?;
        let opts = MolRenderOptions {
            id: entry.id.clone(),
            color: entry.color.clone(),
            atom_shade: entry.shade.as_ref().and_then(|s| s.atoms.clone()),
            bond_shade: entry.shade.as_ref().and_then(|s| s.bonds.clone()),
            mark_atoms: None,
            mark_bonds: None,
            star_labels: entry.star_labels.clone(),
            scale: entry.scale,
            weight: entry.weight,
            align_to: None,
        };
        out.push(render(&mut mol, opts)?);
    }
    Ok(out)
}

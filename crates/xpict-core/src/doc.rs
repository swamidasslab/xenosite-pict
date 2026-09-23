//! Declarative document — plan (pass 1) + paint (pass 2).
//!
//! Hosts call [`plan_edge`] → process the [`EdgePlan`] with RDKit →
//! [`render_doc`] with the [`EdgeResult`]. No RDKit in this module.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cxsmiles::{apply_cx_by_index, cx_source};
use crate::depict::depict_molecule;
use crate::edge::{
    AlignOpts, CoordGenMoleculeResult, EdgePlan, EdgeResult, EdgeTask, EdgeTaskResult,
    MolTemplate,
};
use crate::scene::{MoleculeIn, Scene};

/// Per-atom / per-bond colormap scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadeSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atoms: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonds: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colormap: Option<String>,
    #[serde(default = "default_shade_vmin")]
    pub vmin: f64,
    #[serde(default = "default_shade_vmax")]
    pub vmax: f64,
}

fn default_shade_vmin() -> f64 {
    0.0
}
fn default_shade_vmax() -> f64 {
    1.0
}

impl Default for ShadeSpec {
    fn default() -> Self {
        Self {
            atoms: None,
            bonds: None,
            colormap: None,
            vmin: 0.0,
            vmax: 1.0,
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub star_labels: Option<Vec<Option<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Id of another mol in this group to use as align template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align_to: Option<String>,
    /// Pairs `(query, template)` vs the template; skips MCS when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atom_map: Option<Vec<(u32, u32)>>,
}

fn mol_type() -> String {
    "mol".into()
}

/// Declarative document (`mol` or `group` root).
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align_to: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        atom_map: Option<Vec<(u32, u32)>>,
    },
    Group {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// When true, later children align onto the first (or each `align_to`).
        #[serde(default)]
        align: bool,
        #[serde(default)]
        children: Vec<MolNode>,
    },
}

impl DepictSpec {
    /// Flatten to mol nodes in document order.
    pub fn mols(&self) -> Vec<MolNode> {
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
                align_to,
                atom_map,
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
                align_to: align_to.clone(),
                atom_map: atom_map.clone(),
            }],
            DepictSpec::Group { children, .. } => children.clone(),
        }
    }

    pub fn align_enabled(&self) -> bool {
        matches!(self, DepictSpec::Group { align: true, .. })
    }
}

impl MolNode {
    pub fn structure(&self) -> Result<&str, String> {
        for s in [&self.molfile, &self.cxsmiles, &self.smiles] {
            if let Some(t) = s.as_ref().filter(|x| !x.trim().is_empty()) {
                return Ok(t.as_str());
            }
        }
        Err("mol node needs smiles, cxsmiles, or molfile".into())
    }
}

/// Assign stable unique ids (`user id` or `m_<i>`) in document order.
pub fn assign_mol_ids(mols: &[MolNode]) -> Result<Vec<String>, String> {
    let mut used = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(mols.len());
    for (i, m) in mols.iter().enumerate() {
        let id = match m.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => format!("m_{i}"),
        };
        if !used.insert(id.clone()) {
            return Err(format!("duplicate mol id {id}"));
        }
        out.push(id);
    }
    Ok(out)
}

/// Pass 1: build an [`EdgePlan`] for every mol that needs host coord gen.
///
/// - No align: each mol is its own free-layout root.
/// - `align: true`: forest — default parent = first child; `align_to` overrides.
///
/// Returns `None` only when the document has no molecules.
pub fn plan_edge(spec: &DepictSpec) -> Result<Option<EdgePlan>, String> {
    let mols = spec.mols();
    if mols.is_empty() {
        return Ok(None);
    }
    let ids = assign_mol_ids(&mols)?;
    let id_to_idx: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let align = spec.align_enabled();

    // parent_of[i] = None → root; Some(j) → align onto j
    let mut parent_of: Vec<Option<usize>> = vec![None; mols.len()];
    if align && mols.len() >= 2 {
        parent_of[0] = None;
        for i in 1..mols.len() {
            if let Some(ref target) = mols[i].align_to {
                let j = *id_to_idx
                    .get(target.as_str())
                    .ok_or_else(|| format!("align_to unknown id {target}"))?;
                if j == i {
                    return Err(format!("mol {} cannot align_to itself", ids[i]));
                }
                parent_of[i] = Some(j);
            } else {
                parent_of[i] = Some(0);
            }
        }
    }

    let mut nodes: Vec<MolTemplate> = mols
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let align_opts = parent_of[i].map(|_| AlignOpts {
                atom_map: m.atom_map.clone(),
                min_atoms: None,
            });
            MolTemplate {
                id: ids[i].clone(),
                smiles: m.smiles.clone(),
                cxsmiles: m.cxsmiles.clone(),
                molfile: m.molfile.clone(),
                align: align_opts,
                template_for: vec![],
            }
        })
        .collect();

    // Attach children (drain from leaves upward by building ownership via indices).
    // Collect children indices per parent, then move nodes into place.
    let mut children_of: Vec<Vec<usize>> = vec![vec![]; mols.len()];
    let mut roots_idx: Vec<usize> = Vec::new();
    for (i, p) in parent_of.iter().enumerate() {
        match p {
            None => roots_idx.push(i),
            Some(j) => children_of[*j].push(i),
        }
    }

    fn build_tree(i: usize, nodes: &mut Vec<Option<MolTemplate>>, children_of: &[Vec<usize>]) -> MolTemplate {
        let mut node = nodes[i].take().expect("node");
        for &c in &children_of[i] {
            node.template_for.push(build_tree(c, nodes, children_of));
        }
        node
    }

    let mut slots: Vec<Option<MolTemplate>> = nodes.drain(..).map(Some).collect();
    let roots: Vec<MolTemplate> = roots_idx
        .iter()
        .map(|&i| build_tree(i, &mut slots, &children_of))
        .collect();

    let plan = EdgePlan::new_v1(vec![EdgeTask::CoordGen { roots }]);
    plan.validate()?;
    Ok(Some(plan))
}

fn edge_molecules(edge: &EdgeResult) -> HashMap<&str, &CoordGenMoleculeResult> {
    let mut map = HashMap::new();
    for r in &edge.results {
        let EdgeTaskResult::CoordGen { molecules, .. } = r;
        for m in molecules {
            map.insert(m.id.as_str(), m);
        }
    }
    map
}

/// Apply ``star_labels`` onto ``*`` atoms in layout encounter order.
pub fn apply_star_labels(mol: &mut MoleculeIn, labels: &[Option<String>]) {
    if labels.is_empty() {
        return;
    }
    let stars: Vec<usize> = mol
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, a)| a.z == Some(0) || a.element.as_deref() == Some("*"))
        .map(|(i, _)| i)
        .collect();
    if stars.is_empty() {
        return;
    }
    for (slot, lab) in labels.iter().enumerate() {
        if slot >= stars.len() {
            break;
        }
        let text = match lab {
            None => "*".to_string(),
            Some(s) if s.trim().is_empty() => "*".to_string(),
            Some(s) => s.trim().to_string(),
        };
        if let Some(atom) = mol.atoms.get_mut(stars[slot]) {
            atom.label = Some(text);
        }
    }
}

fn apply_doc_chrome(mut mol: MoleculeIn, node: &MolNode, id: &str) -> MoleculeIn {
    mol.id = Some(id.to_string());
    if let Some(ref c) = node.color {
        mol.color = Some(c.clone());
    }
    if let Some(ref shade) = node.shade {
        mol.atom_shade = shade.atoms.clone();
        mol.bond_shade = shade.bonds.clone();
        mol.shade_vmin = Some(shade.vmin);
        mol.shade_vmax = Some(shade.vmax);
    }
    if let Some(s) = node.scale {
        mol.scale = s;
    }
    if let Some(w) = node.weight {
        mol.weight = w;
    }
    if let Some(ref labels) = node.star_labels {
        apply_star_labels(&mut mol, labels);
    } else if let Some(src) = cx_source(node.cxsmiles.as_deref(), node.smiles.as_deref()) {
        mol = apply_cx_by_index(mol, src);
    }
    mol
}

/// One painted molecule from [`render_doc`] (pass 2).
#[derive(Debug, Clone)]
pub struct DocPaint {
    pub id: String,
    pub molecule: MoleculeIn,
    pub scene: Scene,
}

/// Pass 2: stitch host [`EdgeResult`] coords onto the document and paint.
pub fn render_doc(spec: &DepictSpec, edge: &EdgeResult) -> Result<Vec<DocPaint>, String> {
    let mols = spec.mols();
    let ids = assign_mol_ids(&mols)?;
    let by_id = edge_molecules(edge);
    let mut out = Vec::with_capacity(mols.len());

    for (i, node) in mols.iter().enumerate() {
        let id = &ids[i];
        let row = by_id
            .get(id.as_str())
            .ok_or_else(|| format!("EdgeResult missing molecule id {id}"))?;
        if !row.ok {
            return Err(format!(
                "EdgeResult {id} has ok=false: {}",
                row.error.as_deref().unwrap_or("coord gen failed")
            ));
        }
        let base = row
            .molecule
            .clone()
            .ok_or_else(|| format!("EdgeResult {id} missing molecule"))?;
        let molecule = apply_doc_chrome(base, node, id);
        let scene = depict_molecule(&molecule);
        out.push(DocPaint {
            id: id.clone(),
            molecule,
            scene,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::{CoordMethod, EdgeTaskResult};
    use crate::scene::{AtomIn, BondIn};

    fn ethanol_mol(id: &str) -> MoleculeIn {
        MoleculeIn {
            id: Some(id.into()),
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 0.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 1.5,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
                AtomIn {
                    index: 2,
                    element: Some("O".into()),
                    z: Some(8),
                    x: 2.2,
                    y: 1.0,
                    label: Some("OH".into()),
                    charge: 0,
                },
            ],
            bonds: vec![
                BondIn {
                    index: 0,
                    begin: 0,
                    end: 1,
                    order: 1.0,
                    stereo: None,
                    interior: None,
                },
                BondIn {
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
            scale: 1.0,
            weight: 1.0,
        }
    }

    #[test]
    fn plan_edge_no_align_independent_roots() {
        let spec = DepictSpec::Group {
            id: None,
            align: false,
            children: vec![
                MolNode {
                    smiles: Some("CCO".into()),
                    ..Default::default()
                },
                MolNode {
                    smiles: Some("CCCO".into()),
                    ..Default::default()
                },
            ],
        };
        let plan = plan_edge(&spec).unwrap().unwrap();
        match &plan.tasks[0] {
            EdgeTask::CoordGen { roots } => {
                assert_eq!(roots.len(), 2);
                assert!(roots[0].template_for.is_empty());
                assert!(roots[1].template_for.is_empty());
            }
        }
    }

    #[test]
    fn plan_edge_align_forest_default_first() {
        let spec = DepictSpec::Group {
            id: None,
            align: true,
            children: vec![
                MolNode {
                    id: Some("ref".into()),
                    smiles: Some("c1ccccc1".into()),
                    ..Default::default()
                },
                MolNode {
                    smiles: Some("Cc1ccccc1".into()),
                    atom_map: Some(vec![(1, 0), (2, 1), (3, 2)]),
                    ..Default::default()
                },
            ],
        };
        let plan = plan_edge(&spec).unwrap().unwrap();
        match &plan.tasks[0] {
            EdgeTask::CoordGen { roots } => {
                assert_eq!(roots.len(), 1);
                assert_eq!(roots[0].id, "ref");
                assert_eq!(roots[0].template_for.len(), 1);
                let child = &roots[0].template_for[0];
                assert_eq!(child.id, "m_1");
                assert_eq!(
                    child.align.as_ref().unwrap().atom_map.as_ref().unwrap().len(),
                    3
                );
            }
        }
    }

    #[test]
    fn render_doc_stitches_edge_result() {
        let spec = DepictSpec::Mol {
            smiles: Some("CCO".into()),
            cxsmiles: None,
            molfile: None,
            id: Some("e".into()),
            color: Some("#0b6e4f".into()),
            shade: None,
            star_labels: None,
            scale: None,
            weight: None,
            align_to: None,
            atom_map: None,
        };
        let plan = plan_edge(&spec).unwrap().unwrap();
        let id = match &plan.tasks[0] {
            EdgeTask::CoordGen { roots } => roots[0].id.clone(),
        };
        let edge = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: true,
            molecules: vec![CoordGenMoleculeResult {
                id: id.clone(),
                ok: true,
                method: CoordMethod::Free,
                used_map: None,
                molecule: Some(ethanol_mol(&id)),
                error: None,
            }],
        }]);
    #[test]
    fn render_doc_applies_cx_when_no_star_labels() {
        let spec = DepictSpec::Mol {
            smiles: None,
            cxsmiles: Some("*C |$R1;$|".into()),
            molfile: None,
            id: Some("m".into()),
            color: None,
            shade: None,
            star_labels: None,
            scale: None,
            weight: None,
            align_to: None,
            atom_map: None,
        };
        let mol = MoleculeIn {
            id: Some("m".into()),
            atoms: vec![
                AtomIn {
                    index: 0,
                    element: Some("*".into()),
                    z: Some(0),
                    x: 0.0,
                    y: 0.0,
                    label: Some("*".into()),
                    charge: 0,
                },
                AtomIn {
                    index: 1,
                    element: Some("C".into()),
                    z: Some(6),
                    x: 1.5,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
            ],
            bonds: vec![BondIn {
                index: 0,
                begin: 0,
                end: 1,
                order: 1.0,
                stereo: None,
                interior: None,
            }],
            color: None,
            atom_shade: None,
            bond_shade: None,
            shade_vmin: None,
            shade_vmax: None,
            mark_atoms: vec![],
            mark_bonds: vec![],
            scale: 1.0,
            weight: 1.0,
        };
        let edge = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: true,
            molecules: vec![CoordGenMoleculeResult {
                id: "m".into(),
                ok: true,
                method: CoordMethod::Free,
                used_map: None,
                molecule: Some(mol),
                error: None,
            }],
        }]);
        let painted = render_doc(&spec, &edge).unwrap();
        assert_eq!(painted[0].molecule.atoms[0].label.as_deref(), Some("R1"));
    }
}

//! Declarative document — plan (pass 1) + paint (pass 2) + scheme compose.
//!
//! Wire types live in [`spec`] (opts cascade + DepictSpec collocated).
//! Hosts call [`plan_edge`] → process the [`EdgePlan`] with RDKit →
//! [`render_doc`] with the [`EdgeResult`] → [`compose_scheme`] for reaction
//! schemes. No RDKit in this module.

mod compose;
mod spec;

pub use compose::compose_scheme;
pub use spec::*;

use std::collections::HashMap;

use crate::cxsmiles::{apply_cx_by_index, cx_source};
use crate::depict::depict_molecule;
use crate::edge::{
    AlignOpts, CoordGenMoleculeResult, EdgePlan, EdgeResult, EdgeTask, EdgeTaskResult,
    MolTemplate,
};
use crate::scene::{MoleculeIn, Scene};

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

/// Ensure edge endpoints and label refs resolve to the right node kinds.
pub fn validate_edges(spec: &DepictSpec) -> Result<(), String> {
    // Groups must not contain edge children.
    if matches!(spec, DepictSpec::Group { .. }) {
        for (i, n) in spec.nodes().iter().enumerate() {
            if n.as_edge().is_some() {
                return Err(format!("group children[{i}] cannot be an edge"));
            }
        }
    }

    let mol_ids = assign_mol_ids(&spec.mols())?;
    let mol_set: std::collections::HashSet<&str> =
        mol_ids.iter().map(String::as_str).collect();

    // Collect labelable ids (mol + text).
    let mut labelable = mol_set.clone();
    for t in spec.texts() {
        let Some(id) = t
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if !labelable.insert(id) {
            return Err(format!("duplicate node id {id}"));
        }
    }

    for m in spec.mols() {
        if let Some(ref lab) = m.label {
            for (id, _pos) in lab.placements() {
                let id = id.trim();
                if id.is_empty() {
                    return Err("mol label ref must be non-empty".into());
                }
                match spec.node_by_id(id) {
                    Some(Node::Text(_)) => {}
                    Some(Node::Mol(_)) => {
                        return Err(format!("mol label {id} must ref a text node, not a mol"));
                    }
                    Some(Node::Edge(_)) => {
                        return Err(format!("mol label {id} must ref a text node, not an edge"));
                    }
                    None => return Err(format!("mol label unknown text id {id}")),
                }
            }
        }
    }

    for e in spec.edges() {
        if e.sources.is_empty() {
            return Err("edge sources must be non-empty".into());
        }
        if e.targets.is_empty() {
            return Err("edge targets must be non-empty".into());
        }
        for id in e.sources.iter() {
            let id = id.trim();
            if id.is_empty() {
                return Err("edge source id must be non-empty".into());
            }
            if !mol_set.contains(id) {
                return Err(format!("edge source unknown id {id}"));
            }
        }
        for id in e.targets.iter() {
            let id = id.trim();
            if id.is_empty() {
                return Err("edge target id must be non-empty".into());
            }
            if !mol_set.contains(id) {
                return Err(format!("edge target unknown id {id}"));
            }
        }
        if let Some(ref lab) = e.label {
            for (id, pos) in lab.placements() {
                let id = id.trim();
                if id.is_empty() {
                    return Err("edge label ref must be non-empty".into());
                }
                match spec.node_by_id(id) {
                    Some(Node::Text(_)) | Some(Node::Mol(_)) => {}
                    Some(Node::Edge(_)) => {
                        return Err(format!("edge label ({pos:?}) id {id} cannot ref an edge"));
                    }
                    None => return Err(format!("edge label ({pos:?}) unknown id {id}")),
                }
            }
        }
    }
    Ok(())
}

/// Pass 1: build an [`EdgePlan`] for every mol that needs host coord gen.
///
/// - No align: each mol is its own free-layout root.
/// - `align: true`: forest — default parent = first child; `align_to` overrides.
///
/// Returns `None` only when the document has no molecules.
pub fn plan_edge(spec: &DepictSpec) -> Result<Option<EdgePlan>, String> {
    validate_edges(spec)?;
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
                let name = target.ref_id();
                let j = *id_to_idx
                    .get(name)
                    .ok_or_else(|| format!("align_to unknown id {name}"))?;
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
            let align_opts = parent_of[i].map(|_| match &m.align_to {
                Some(a) => a.align_opts(),
                None => AlignOpts::default(),
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

fn apply_doc_chrome(
    mut mol: MoleculeIn,
    node: &MolNode,
    id: &str,
    chrome: &MolOpts,
) -> MoleculeIn {
    mol.id = Some(id.to_string());
    if let Some(ref c) = chrome.color {
        mol.color = Some(c.clone());
    }
    // Scores always from the node (non-cascading); window from resolved chrome.
    if let Some(ref shade) = node.shade {
        mol.atom_shade = shade.atoms.clone();
        mol.bond_shade = shade.bonds.clone();
    }
    if let Some(ref style) = chrome.shade {
        if let Some(v) = style.vmin {
            mol.shade_vmin = Some(v);
        } else if let Some(ref shade) = node.shade {
            mol.shade_vmin = Some(shade.vmin);
        }
        if let Some(v) = style.vmax {
            mol.shade_vmax = Some(v);
        } else if let Some(ref shade) = node.shade {
            mol.shade_vmax = Some(shade.vmax);
        }
    } else if let Some(ref shade) = node.shade {
        mol.shade_vmin = Some(shade.vmin);
        mol.shade_vmax = Some(shade.vmax);
    }
    if let Some(s) = chrome.scale {
        mol.scale = s;
    }
    if let Some(w) = chrome.weight {
        mol.weight = w;
    }
    // halo reserved on chrome for future paint; MoleculeIn has no halo flag yet.
    let _ = chrome.halo;
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
        let chrome = spec.resolve_mol_chrome(i);
        let molecule = apply_doc_chrome(base, node, id, &chrome);
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
                Node::Mol(MolNode {
                    smiles: Some("CCO".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    smiles: Some("CCCO".into()),
                    ..Default::default()
                }),
            ],
                    opts: None,
            color: None,
            scale: None,
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
                Node::Mol(MolNode {
                    id: Some("ref".into()),
                    smiles: Some("c1ccccc1".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    smiles: Some("Cc1ccccc1".into()),
                    align_to: Some(AlignTo::Spec(AlignToSpec {
                        ref_id: "ref".into(),
                        atom_map: Some(vec![(1, 0), (2, 1), (3, 2)]),
                        min_atoms: None,
                    })),
                    ..Default::default()
                }),
            ],
            opts: None,
            color: None,
            scale: None,
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
            label: None,
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
                    halo: None,
            opts: None,
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
        let painted = render_doc(&spec, &edge).unwrap();
        assert_eq!(painted.len(), 1);
        assert!(painted[0].scene.width > 0.0);
        assert_eq!(painted[0].molecule.color.as_deref(), Some("#0b6e4f"));
    }

    #[test]
    fn render_doc_applies_cx_when_no_star_labels() {
        let spec = DepictSpec::Mol {
            label: None,
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
                    halo: None,
            opts: None,
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

    #[test]
    fn structure_prefers_molfile_then_cx_then_smiles() {
        let mut n = MolNode {
            smiles: Some("CCO".into()),
            cxsmiles: Some("CCO |$;$|".into()),
            molfile: Some("molblock".into()),
            ..Default::default()
        };
        assert_eq!(n.structure().unwrap(), "molblock");
        n.molfile = None;
        assert_eq!(n.structure().unwrap(), "CCO |$;$|");
        n.cxsmiles = None;
        assert_eq!(n.structure().unwrap(), "CCO");
        n.smiles = Some("  ".into());
        assert!(n.structure().unwrap_err().contains("needs smiles"));
    }

    #[test]
    fn assign_mol_ids_rejects_duplicates_and_trims() {
        let mols = vec![
            MolNode {
                id: Some("  a  ".into()),
                smiles: Some("C".into()),
                ..Default::default()
            },
            MolNode {
                id: Some("a".into()),
                smiles: Some("CC".into()),
                ..Default::default()
            },
        ];
        assert!(assign_mol_ids(&mols).unwrap_err().contains("duplicate"));
        let auto = assign_mol_ids(&[
            MolNode {
                smiles: Some("C".into()),
                ..Default::default()
            },
            MolNode {
                id: Some("".into()),
                smiles: Some("CC".into()),
                ..Default::default()
            },
        ])
        .unwrap();
        assert_eq!(auto, ["m_0", "m_1"]);
    }

    #[test]
    fn plan_edge_empty_group_is_none() {
        let spec = DepictSpec::Group {
            id: None,
            align: false,
            children: vec![],
                    opts: None,
            color: None,
            scale: None,
        };
        assert!(plan_edge(&spec).unwrap().is_none());
    }

    #[test]
    fn plan_edge_align_to_unknown_and_self_error() {
        let unknown = DepictSpec::Group {
            id: None,
            align: true,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("a".into()),
                    smiles: Some("C".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    smiles: Some("CC".into()),
                    align_to: Some(AlignTo::Ref("missing".into())),
                    ..Default::default()
                }),
            ],
                    opts: None,
            color: None,
            scale: None,
        };
        assert!(plan_edge(&unknown).unwrap_err().contains("unknown id"));

        let self_align = DepictSpec::Group {
            id: None,
            align: true,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("a".into()),
                    smiles: Some("C".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("b".into()),
                    smiles: Some("CC".into()),
                    align_to: Some(AlignTo::Ref("b".into())),
                    ..Default::default()
                }),
            ],
                    opts: None,
            color: None,
            scale: None,
        };
        assert!(plan_edge(&self_align).unwrap_err().contains("itself"));
    }

    #[test]
    fn plan_edge_align_to_explicit_parent() {
        let spec = DepictSpec::Group {
            id: None,
            align: true,
            children: vec![
                Node::Mol(MolNode {
                    id: Some("left".into()),
                    smiles: Some("C".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("mid".into()),
                    smiles: Some("CC".into()),
                    ..Default::default()
                }),
                Node::Mol(MolNode {
                    id: Some("q".into()),
                    smiles: Some("CCC".into()),
                    align_to: Some(AlignTo::Ref("mid".into())),
                    ..Default::default()
                }),
            ],
                    opts: None,
            color: None,
            scale: None,
        };
        let plan = plan_edge(&spec).unwrap().unwrap();
        match &plan.tasks[0] {
            EdgeTask::CoordGen { roots } => {
                assert_eq!(roots.len(), 1);
                assert_eq!(roots[0].id, "left");
                assert_eq!(roots[0].template_for.len(), 1);
                assert_eq!(roots[0].template_for[0].id, "mid");
                assert_eq!(roots[0].template_for[0].template_for[0].id, "q");
            }
        }
    }

    #[test]
    fn apply_star_labels_covers_null_empty_and_overflow() {
        let mut mol = MoleculeIn {
            id: None,
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
                    x: 1.0,
                    y: 0.0,
                    label: None,
                    charge: 0,
                },
                AtomIn {
                    index: 2,
                    element: Some("*".into()),
                    z: Some(0),
                    x: 2.0,
                    y: 0.0,
                    label: Some("*".into()),
                    charge: 0,
                },
            ],
            bonds: vec![],
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
        apply_star_labels(&mut mol, &[]);
        assert_eq!(mol.atoms[0].label.as_deref(), Some("*"));
        apply_star_labels(
            &mut mol,
            &[None, Some("  ".into()), Some("R2".into()), Some("extra".into())],
        );
        // Two stars: slot0 → "*", slot1 empty → "*"; R2/extra overflow ignored.
        assert_eq!(mol.atoms[0].label.as_deref(), Some("*"));
        assert_eq!(mol.atoms[2].label.as_deref(), Some("*"));
    }

    #[test]
    fn render_doc_star_labels_shade_scale_weight() {
        let spec = DepictSpec::Mol {
            label: None,
            smiles: Some("*C".into()),
            cxsmiles: None,
            molfile: None,
            id: Some("s".into()),
            color: None,
            shade: Some(ShadeSpec {
                atoms: Some(vec![0.2, 0.8]),
                bonds: Some(vec![0.5]),
                colormap: Some("xenosite".into()),
                vmin: 0.0,
                vmax: 1.0,
            }),
            star_labels: Some(vec![Some("R1".into())]),
            scale: Some(1.5),
            weight: Some(1.2),
            align_to: None,
                    halo: None,
            opts: None,
        };
        let mut mol = ethanol_mol("s");
        mol.atoms[0].element = Some("*".into());
        mol.atoms[0].z = Some(0);
        mol.atoms[0].label = Some("*".into());
        let edge = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: true,
            molecules: vec![CoordGenMoleculeResult {
                id: "s".into(),
                ok: true,
                method: CoordMethod::Free,
                used_map: None,
                molecule: Some(mol),
                error: None,
            }],
        }]);
        let painted = render_doc(&spec, &edge).unwrap();
        let m = &painted[0].molecule;
        assert_eq!(m.atoms[0].label.as_deref(), Some("R1"));
        assert_eq!(m.atom_shade.as_ref().unwrap()[0], 0.2);
        assert_eq!(m.scale, 1.5);
        assert!((m.weight - 1.2).abs() < 1e-12);
    }

    #[test]
    fn render_doc_errors_on_missing_or_failed_edge_row() {
        let spec = DepictSpec::Mol {
            label: None,
            smiles: Some("C".into()),
            cxsmiles: None,
            molfile: None,
            id: Some("x".into()),
            color: None,
            shade: None,
            star_labels: None,
            scale: None,
            weight: None,
            align_to: None,
                    halo: None,
            opts: None,
        };
        let missing = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: false,
            molecules: vec![],
        }]);
        assert!(render_doc(&spec, &missing).unwrap_err().contains("missing"));

        let failed = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: false,
            molecules: vec![CoordGenMoleculeResult {
                id: "x".into(),
                ok: false,
                method: CoordMethod::None,
                used_map: None,
                molecule: None,
                error: Some("boom".into()),
            }],
        }]);
        assert!(render_doc(&spec, &failed).unwrap_err().contains("ok=false"));

        let no_mol = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: true,
            molecules: vec![CoordGenMoleculeResult {
                id: "x".into(),
                ok: true,
                method: CoordMethod::Free,
                used_map: None,
                molecule: None,
                error: None,
            }],
        }]);
        assert!(render_doc(&spec, &no_mol).unwrap_err().contains("missing molecule"));
    }

    #[test]
    fn shade_spec_default_and_group_align_flag() {
        let s = ShadeSpec::default();
        assert_eq!(s.vmin, 0.0);
        assert_eq!(s.vmax, 1.0);
        let g = DepictSpec::Group {
            id: Some("g".into()),
            align: true,
            children: vec![],
                    opts: None,
            color: None,
            scale: None,
        };
        assert!(g.align_enabled());
        assert!(g.mols().is_empty());
    }

    #[test]
    fn align_to_json_string_or_object() {
        let s: AlignTo = serde_json::from_str(r#""ref""#).unwrap();
        assert_eq!(s.ref_id(), "ref");
        let o: AlignTo = serde_json::from_str(
            r#"{"ref":"mid","atom_map":[[1,0],[2,1]],"min_atoms":4}"#,
        )
        .unwrap();
        assert_eq!(o.ref_id(), "mid");
        let opts = o.align_opts();
        assert_eq!(opts.atom_map.as_ref().unwrap().len(), 2);
        assert_eq!(opts.min_atoms, Some(4));
    }

    #[test]
    fn opts_patch_json_discriminated_and_list() {
        // Universal singleton
        let u: Opts = serde_json::from_str(r##"{"color":"#111"}"##).unwrap();
        match &u {
            Opts::One(OptsPatch::Universal(c)) => assert_eq!(c.color.as_deref(), Some("#111")),
            _ => panic!("expected universal"),
        }
        // Typed mol patch
        let t: OptsPatch = serde_json::from_str(r#"{"type":"mol","weight":1.5,"halo":false}"#).unwrap();
        match t {
            OptsPatch::Typed(TypedOptsPatch::Mol { opts }) => {
                assert_eq!(opts.weight, Some(1.5));
                assert_eq!(opts.halo, Some(false));
            }
            _ => panic!("expected typed mol"),
        }
        // for_types multi
        let f: OptsPatch =
            serde_json::from_str(r##"{"for_types":["mol","group"],"color":"#0b6e4f"}"##).unwrap();
        match f {
            OptsPatch::ForTypes(p) => {
                assert_eq!(p.for_types, vec![NodeType::Mol, NodeType::Group]);
                assert_eq!(p.opts.color.as_deref(), Some("#0b6e4f"));
            }
            _ => panic!("expected for_types"),
        }
        // List container
        let list: Opts = serde_json::from_str(
            r##"[{"color":"#111"},{"type":"mol","weight":1.2},{"for_types":["mol"],"scale":2.0}]"##,
        )
        .unwrap();
        assert!(matches!(list, Opts::Many(ref v) if v.len() == 3));
    }

    #[test]
    fn cascade_group_opts_list_inherits_then_child_overrides() {
        let spec: DepictSpec = serde_json::from_str(
            r##"{
              "type": "group",
              "align": false,
              "opts": [
                {"color": "#111", "scale": 1.0},
                {"type": "mol", "weight": 1.0, "halo": true},
                {"type": "mol", "weight": 1.25},
                {"for_types": ["mol"], "scale": 1.5}
              ],
              "children": [
                {
                  "type": "mol",
                  "id": "a",
                  "smiles": "CCO",
                  "opts": {"type": "mol", "color": "#0b6e4f"}
                },
                {
                  "type": "mol",
                  "id": "b",
                  "smiles": "CCC",
                  "weight": 2.0,
                  "opts": [
                    {"type": "mol", "halo": false},
                    {"for_types": ["group"], "color": "#fff"}
                  ]
                }
              ]
            }"##,
        )
        .unwrap();

        let a = spec.resolve_mol_chrome(0);
        assert_eq!(a.color.as_deref(), Some("#0b6e4f")); // child typed patch wins
        assert_eq!(a.weight, Some(1.25)); // later typed patch in group list wins
        assert_eq!(a.scale, Some(1.5)); // later for_types common key overrides universal
        assert_eq!(a.halo, Some(true));

        let b = spec.resolve_mol_chrome(1);
        assert_eq!(b.color.as_deref(), Some("#111")); // group universal; child for_types group ignored
        assert_eq!(b.weight, Some(2.0)); // local flat last
        assert_eq!(b.halo, Some(false)); // child opts list
        assert_eq!(b.scale, Some(1.5)); // inherited from group for_types
    }

    #[test]
    fn cascade_group_flat_overrides_group_opts_list() {
        let spec: DepictSpec = serde_json::from_str(
            r##"{
              "type": "group",
              "opts": [{"color": "#111", "scale": 1.0}],
              "color": "#222",
              "children": [{"type": "mol", "smiles": "C", "scale": 3.0}]
            }"##,
        )
        .unwrap();
        let chrome = spec.resolve_mol_chrome(0);
        assert_eq!(chrome.color.as_deref(), Some("#222")); // group flat after opts list
        assert_eq!(chrome.scale, Some(3.0)); // node local last
    }

    #[test]
    fn cascade_shade_style_deep_merges_across_list() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "group",
              "opts": [
                {"type": "mol", "shade": {"vmin": 0.0, "vmax": 1.0}},
                {"type": "mol", "shade": {"vmax": 0.5, "colormap": "xenosite"}}
              ],
              "children": [
                {
                  "type": "mol",
                  "smiles": "C",
                  "shade": {"atoms": [0.2], "vmin": -1.0, "vmax": 1.0}
                }
              ]
            }"#,
        )
        .unwrap();
        let chrome = spec.resolve_mol_chrome(0);
        let s = chrome.shade.as_ref().unwrap();
        // Local shade.style() merges last → vmin/vmax from node ShadeSpec
        assert_eq!(s.vmin, Some(-1.0));
        assert_eq!(s.vmax, Some(1.0));
        // colormap from group list still present unless local cleared it
        assert_eq!(s.colormap.as_deref(), Some("xenosite"));
    }

    #[test]
    fn cascade_typed_group_patch_does_not_apply_to_mol() {
        let spec: DepictSpec = serde_json::from_str(
            r##"{
              "type": "group",
              "opts": [
                {"type": "group", "color": "#ff0000"},
                {"type": "mol", "color": "#00ff00"}
              ],
              "children": [{"type": "mol", "smiles": "C"}]
            }"##,
        )
        .unwrap();
        let chrome = spec.resolve_mol_chrome(0);
        assert_eq!(chrome.color.as_deref(), Some("#00ff00"));
    }

    #[test]
    fn render_doc_uses_cascaded_color_from_group_opts_list() {
        let spec: DepictSpec = serde_json::from_str(
            r##"{
              "type": "group",
              "opts": [{"color": "#abcdef"}, {"type": "mol", "weight": 1.5}],
              "children": [{"type": "mol", "id": "e", "smiles": "CCO"}]
            }"##,
        )
        .unwrap();
        let edge = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: true,
            molecules: vec![CoordGenMoleculeResult {
                id: "e".into(),
                ok: true,
                method: CoordMethod::Free,
                used_map: None,
                molecule: Some(ethanol_mol("e")),
                error: None,
            }],
        }]);
        let painted = render_doc(&spec, &edge).unwrap();
        assert_eq!(painted[0].molecule.color.as_deref(), Some("#abcdef"));
        assert!((painted[0].molecule.weight - 1.5).abs() < 1e-12);
    }

    #[test]
    fn reaction_scheme_nodes_and_edges_json() {
        let spec: DepictSpec = serde_json::from_str(
            r##"{
              "type": "reaction_scheme",
              "opts": [{"color": "#111"}, {"type": "mol", "weight": 1.2}],
              "children": [
                {"type": "text", "id": "adh", "text": "ADH"},
                {"type": "text", "id": "aldh", "text": "ALDH"},
                {"type": "mol", "id": "a", "smiles": "CCO", "label": "adh"},
                {
                  "type": "edge",
                  "source": "a",
                  "target": "b",
                  "label": {
                    "above": ["adh"],
                    "below": ["rt"],
                    "left": ["nabh4"]
                  },
                  "arrow": "forward"
                },
                {"type": "text", "id": "rt", "text": "rt"},
                {"type": "mol", "id": "nabh4", "smiles": "[BH4-]", "scale": 0.4},
                {"type": "mol", "id": "b", "smiles": "CC=O"},
                {
                  "type": "edge",
                  "source": "b",
                  "target": "c",
                  "label": [
                    "aldh",
                    {"id": "nad", "pos": "right"}
                  ],
                  "arrow": "equilibrium",
                  "color": "#064",
                  "dashed": true
                },
                {"type": "text", "id": "nad", "text": "NAD+"},
                {"type": "mol", "id": "c", "smiles": "CC(=O)O"}
              ]
            }"##,
        )
        .unwrap();

        assert_eq!(spec.mols().len(), 4);
        assert_eq!(spec.texts().len(), 4);
        assert_eq!(spec.nodes().len(), 10);
        let edges = spec.edges();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].sources.as_slice(), &["a".to_string()]);
        assert_eq!(edges[0].targets.as_slice(), &["b".to_string()]);
        let e0 = edges[0].label.as_ref().unwrap().placements();
        assert_eq!(
            e0,
            vec![
                ("adh".into(), crate::doc::LabelPos::Above),
                ("rt".into(), crate::doc::LabelPos::Below),
                ("nabh4".into(), crate::doc::LabelPos::Left),
            ]
        );
        assert_eq!(edges[0].arrow, EdgeArrow::Forward);
        assert_eq!(edges[1].arrow, EdgeArrow::Equilibrium);
        let e1 = edges[1].label.as_ref().unwrap().placements();
        assert_eq!(
            e1,
            vec![
                ("aldh".into(), crate::doc::LabelPos::Above),
                ("nad".into(), crate::doc::LabelPos::Right),
            ]
        );
        assert_eq!(edges[1].color.as_deref(), Some("#064"));
        assert!(edges[1].dashed);
        assert_eq!(
            spec.mols()[0].label.as_ref().unwrap().placements(),
            vec![("adh".into(), crate::doc::LabelPos::Above)]
        );
        validate_edges(&spec).unwrap();

        let chrome = spec.resolve_mol_chrome(0);
        assert_eq!(chrome.color.as_deref(), Some("#111"));
        assert_eq!(chrome.weight, Some(1.2));
    }

    #[test]
    fn group_mol_label_refs_text_node() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "group",
              "children": [
                {"type": "text", "id": "cap", "text": "ethanol"},
                {"type": "mol", "id": "a", "smiles": "CCO", "label": "cap"}
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(spec.mols().len(), 1);
        assert_eq!(spec.texts().len(), 1);
        assert_eq!(
            spec.mols()[0].label.as_ref().unwrap().placements(),
            vec![("cap".into(), LabelPos::Above)]
        );
        validate_edges(&spec).unwrap();
    }

    #[test]
    fn edge_label_string_list_and_placed_json() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "children": [
                {"type": "text", "id": "adh", "text": "ADH"},
                {"type": "text", "id": "rt", "text": "rt"},
                {"type": "mol", "id": "a", "smiles": "C"},
                {"type": "mol", "id": "b", "smiles": "CC"},
                {
                  "type": "edge",
                  "source": "a",
                  "target": "b",
                  "label": "adh"
                },
                {
                  "type": "edge",
                  "source": "b",
                  "target": "a",
                  "label": {"id": "rt", "pos": "below"}
                }
              ]
            }"#,
        )
        .unwrap();
        let edges = spec.edges();
        assert_eq!(
            edges[0].label.as_ref().unwrap().placements(),
            vec![("adh".into(), LabelPos::Above)]
        );
        assert_eq!(
            edges[1].label.as_ref().unwrap().placements(),
            vec![("rt".into(), LabelPos::Below)]
        );
        validate_edges(&spec).unwrap();
    }

    #[test]
    fn reaction_scheme_edge_unknown_id_errors() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "children": [
                {"type": "mol", "id": "a", "smiles": "C"},
                {"type": "edge", "source": "a", "target": "missing"}
              ]
            }"#,
        )
        .unwrap();
        let err = validate_edges(&spec).unwrap_err();
        assert!(err.contains("unknown id missing"));
        assert!(plan_edge(&spec).unwrap_err().contains("unknown id"));
    }

    #[test]
    fn mol_label_unknown_text_id_errors() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "group",
              "children": [
                {"type": "mol", "id": "a", "smiles": "C", "label": "nope"}
              ]
            }"#,
        )
        .unwrap();
        let err = validate_edges(&spec).unwrap_err();
        assert!(err.contains("unknown text id nope"));
    }

    #[test]
    fn reaction_scheme_layout_defaults_and_overrides() {
        let bare: DepictSpec = serde_json::from_str(
            r#"{"type":"reaction_scheme","children":[{"type":"mol","id":"a","smiles":"C"}]}"#,
        )
        .unwrap();
        let layout = match &bare {
            DepictSpec::ReactionScheme { layout, .. } => layout.clone().unwrap_or_default(),
            _ => panic!("expected reaction_scheme"),
        };
        assert_eq!(layout.direction_or_default(), LayoutDirection::Right);
        assert_eq!(layout.edge_routing_or_default(), EdgeRouting::Polyline);
        assert_eq!(layout.algorithm_or_default(), LayoutAlgorithm::Layered);

        let custom: DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "layout": {
                "direction": "down",
                "edge_routing": "orthogonal",
                "algorithm": "force"
              },
              "children": [{"type":"mol","id":"a","smiles":"C"}]
            }"#,
        )
        .unwrap();
        let layout = match &custom {
            DepictSpec::ReactionScheme {
                layout: Some(l), ..
            } => l,
            _ => panic!("expected layout"),
        };
        assert_eq!(layout.direction, Some(LayoutDirection::Down));
        assert_eq!(layout.edge_routing, Some(EdgeRouting::Orthogonal));
        assert_eq!(layout.algorithm, Some(LayoutAlgorithm::Force));
    }

    #[test]
    fn edge_routing_overrides_scheme_layout() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "layout": { "edge_routing": "polyline" },
              "children": [
                {"type": "mol", "id": "a", "smiles": "C"},
                {"type": "mol", "id": "b", "smiles": "CC"},
                {
                  "type": "edge",
                  "source": "a",
                  "target": "b",
                  "edge_routing": "orthogonal"
                },
                {
                  "type": "edge",
                  "source": "b",
                  "target": "a"
                }
              ]
            }"#,
        )
        .unwrap();
        let layout = match &spec {
            DepictSpec::ReactionScheme {
                layout: Some(l), ..
            } => l.clone(),
            _ => LayoutOpts::default(),
        };
        let edges = spec.edges();
        assert_eq!(edges[0].edge_routing, Some(EdgeRouting::Orthogonal));
        assert_eq!(edges[0].edge_routing_or(&layout), EdgeRouting::Orthogonal);
        assert_eq!(edges[1].edge_routing, None);
        assert_eq!(edges[1].edge_routing_or(&layout), EdgeRouting::Polyline);
    }

    #[test]
    fn edge_multi_reactants_and_products() {
        let spec: DepictSpec = serde_json::from_str(
            r#"{
              "type": "reaction_scheme",
              "children": [
                {"type": "mol", "id": "a", "smiles": "C"},
                {"type": "mol", "id": "b", "smiles": "O"},
                {"type": "mol", "id": "c", "smiles": "CO"},
                {"type": "mol", "id": "d", "smiles": "O=O"},
                {
                  "type": "edge",
                  "sources": ["a", "b"],
                  "targets": ["c", "d"],
                  "arrow": "forward"
                }
              ]
            }"#,
        )
        .unwrap();
        let e = &spec.edges()[0];
        assert_eq!(
            e.sources.as_slice(),
            &["a".to_string(), "b".to_string()]
        );
        assert_eq!(
            e.targets.as_slice(),
            &["c".to_string(), "d".to_string()]
        );
        validate_edges(&spec).unwrap();
    }

    #[test]
    fn cascade_reaction_scheme_typed_patch_does_not_paint_mol() {
        let spec: DepictSpec = serde_json::from_str(
            r##"{
              "type": "reaction_scheme",
              "opts": [
                {"type": "reaction_scheme", "color": "#ff0000"},
                {"type": "mol", "color": "#00ff00"}
              ],
              "children": [{"type": "mol", "id": "n", "smiles": "C"}]
            }"##,
        )
        .unwrap();
        let chrome = spec.resolve_mol_chrome(0);
        assert_eq!(chrome.color.as_deref(), Some("#00ff00"));
    }
}

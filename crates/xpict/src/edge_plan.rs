//! Process [`xpict_core::EdgePlan`] with native RDKit Depictor.
//!
//! Chematic MCS runs in [`xpict_core::resolve_edge_plan_maps`] /
//! [`xpict_core::plan_edge`]. This processor only applies explicit atom maps
//! to Depictor.

use std::collections::HashMap;

use xpict_core::edge::{
    resolve_edge_plan_maps, CoordGenMoleculeResult, CoordMethod, EdgePlan, EdgeResult, EdgeTask,
    EdgeTaskResult, MolTemplate, MIN_MCS_ATOMS,
};
use xpict_core::AlignOpts;

use crate::layout::layout_with_rdkit_meta;
use crate::Error;

/// Build a two-node align forest (template root + query child).
pub fn build_align_plan(
    template_source: &str,
    query_source: &str,
    atom_map: Option<Vec<(u32, u32)>>,
) -> EdgePlan {
    let mut plan = EdgePlan::new_v1(vec![EdgeTask::CoordGen {
        roots: vec![MolTemplate {
            id: "m_0".into(),
            smiles: Some(template_source.into()),
            cxsmiles: None,
            molfile: None,
            align: None,
            template_for: vec![MolTemplate {
                id: "m_1".into(),
                smiles: Some(query_source.into()),
                cxsmiles: None,
                molfile: None,
                align: Some(AlignOpts {
                    atom_map,
                    ..Default::default()
                }),
                template_for: vec![],
            }],
        }],
    }]);
    resolve_edge_plan_maps(&mut plan);
    plan
}

fn source_of(node: &MolTemplate) -> Result<String, Error> {
    node.source()
        .map(str::to_string)
        .map_err(Error::Parse)
}

/// Run all ``coord_gen`` tasks; always return flat molecule rows.
///
/// Align failure → automatic free (unaligned) layout; ``method: None``,
/// ``ok: true`` when coords were produced.
pub fn process_edge_plan(plan: &EdgePlan) -> Result<EdgeResult, Error> {
    Ok(process_edge_plan_with_frames(plan)?.0)
}

/// Like [`process_edge_plan`], plus pose molblocks keyed by mol id (for
/// [`crate::Rendered::frame_molblock`]).
pub fn process_edge_plan_with_frames(
    plan: &EdgePlan,
) -> Result<(EdgeResult, HashMap<String, String>), Error> {
    plan.validate().map_err(Error::Parse)?;
    let mut plan = plan.clone();
    resolve_edge_plan_maps(&mut plan);

    let mut task_results = Vec::new();
    let mut all_poses: HashMap<String, String> = HashMap::new();

    for task in &plan.tasks {
        match task {
            EdgeTask::CoordGen { roots } => {
                let mut rows: Vec<CoordGenMoleculeResult> = Vec::new();
                let mut poses: HashMap<String, String> = HashMap::new();

                for root in roots {
                    visit(root, None, &mut rows, &mut poses)?;
                }

                all_poses.extend(poses);
                let ok = rows.iter().all(|r| r.ok);
                task_results.push(EdgeTaskResult::CoordGen {
                    ok,
                    molecules: rows,
                });
            }
        }
    }

    Ok((EdgeResult::new_v1(task_results), all_poses))
}

fn free_layout(
    source: &str,
    id: &str,
) -> Result<(xpict_core::MoleculeIn, String), Error> {
    let (molecule, pose, _) =
        layout_with_rdkit_meta(source, None, None, Some(id.to_string()))?;
    Ok((molecule, pose))
}

fn visit(
    node: &MolTemplate,
    parent_id: Option<&str>,
    rows: &mut Vec<CoordGenMoleculeResult>,
    poses: &mut HashMap<String, String>,
) -> Result<(), Error> {
    let source = source_of(node)?;
    let min_atoms = node
        .align
        .as_ref()
        .and_then(|a| a.min_atoms)
        .unwrap_or(MIN_MCS_ATOMS);
    let explicit_map = node.align.as_ref().and_then(|a| a.atom_map.clone());
    let map_from_mcs = node.align.as_ref().is_some_and(|a| a.map_from_mcs);

    match parent_id {
        None => match free_layout(&source, &node.id) {
            Ok((molecule, pose)) => {
                rows.push(CoordGenMoleculeResult {
                    id: node.id.clone(),
                    ok: true,
                    method: CoordMethod::Free,
                    used_map: None,
                    molecule: Some(molecule),
                    error: None,
                });
                poses.insert(node.id.clone(), pose);
            }
            Err(e) => {
                rows.push(CoordGenMoleculeResult {
                    id: node.id.clone(),
                    ok: false,
                    method: CoordMethod::None,
                    used_map: None,
                    molecule: None,
                    error: Some(e.to_string()),
                });
                return Ok(());
            }
        },
        Some(pid) => {
            let template = match poses.get(pid) {
                Some(p) => p.clone(),
                None => {
                    match free_layout(&source, &node.id) {
                        Ok((molecule, pose)) => {
                            rows.push(CoordGenMoleculeResult {
                                id: node.id.clone(),
                                ok: true,
                                method: CoordMethod::None,
                                used_map: None,
                                molecule: Some(molecule),
                                error: Some(format!(
                                    "missing parent pose {pid}; fell back to unaligned coord gen"
                                )),
                            });
                            poses.insert(node.id.clone(), pose);
                        }
                        Err(e) => {
                            rows.push(CoordGenMoleculeResult {
                                id: node.id.clone(),
                                ok: false,
                                method: CoordMethod::None,
                                used_map: None,
                                molecule: None,
                                error: Some(e.to_string()),
                            });
                        }
                    }
                    for child in &node.template_for {
                        visit(child, Some(node.id.as_str()), rows, poses)?;
                    }
                    return Ok(());
                }
            };

            let use_map = explicit_map.filter(|m| (m.len() as u32) >= min_atoms);
            let method_if_ok = if use_map.is_some() {
                if map_from_mcs {
                    CoordMethod::Mcs
                } else {
                    CoordMethod::AtomMap
                }
            } else {
                CoordMethod::None
            };

            match layout_with_rdkit_meta(
                &source,
                Some(&template),
                use_map.as_deref(),
                Some(node.id.clone()),
            ) {
                Ok((molecule, pose, matched)) if matched => {
                    rows.push(CoordGenMoleculeResult {
                        id: node.id.clone(),
                        ok: true,
                        method: method_if_ok,
                        used_map: use_map,
                        molecule: Some(molecule),
                        error: None,
                    });
                    poses.insert(node.id.clone(), pose);
                }
                Ok(_) | Err(_) => match free_layout(&source, &node.id) {
                    Ok((mol, pose2)) => {
                        rows.push(CoordGenMoleculeResult {
                            id: node.id.clone(),
                            ok: true,
                            method: CoordMethod::None,
                            used_map: None,
                            molecule: Some(mol),
                            error: Some(
                                "align failed; fell back to unaligned coord gen".into(),
                            ),
                        });
                        poses.insert(node.id.clone(), pose2);
                    }
                    Err(e) => {
                        rows.push(CoordGenMoleculeResult {
                            id: node.id.clone(),
                            ok: false,
                            method: CoordMethod::None,
                            used_map: None,
                            molecule: None,
                            error: Some(e.to_string()),
                        });
                    }
                },
            }
        }
    }

    for child in &node.template_for {
        visit(child, Some(node.id.as_str()), rows, poses)?;
    }
    Ok(())
}

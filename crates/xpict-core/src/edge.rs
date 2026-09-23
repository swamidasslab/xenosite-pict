//! EdgePlan / EdgeResult — host callback ABI (coord_gen / align).
//!
//! Core builds the plan (ids + forest); language edges process it and return
//! flat [`MoleculeIn`] results. No molblocks on this wire.

use serde::{Deserialize, Serialize};

use crate::scene::MoleculeIn;

#[cfg(feature = "codegen")]
use schemars::JsonSchema;
#[cfg(feature = "codegen")]
use ts_rs::TS;

/// Minimum mapped atoms before align is trusted (MCS or explicit map).
pub const MIN_MCS_ATOMS: u32 = 3;

/// Align this mol onto its parent template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct AlignOpts {
    /// Pairs `(query_atom, template_atom)`. `None` → edge runs MCS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub atom_map: Option<Vec<(u32, u32)>>,
    /// Override [`MIN_MCS_ATOMS`] when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub min_atoms: Option<u32>,
}

/// One node in a coord_gen forest (root = free layout; children align to parent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct MolTemplate {
    /// Rust-assigned unique id; round-trips to the document node.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub smiles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub cxsmiles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub molfile: Option<String>,
    /// Opts for aligning onto the parent; `None` on roots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub align: Option<AlignOpts>,
    /// Children that use this node as their align template.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_for: Vec<MolTemplate>,
}

impl MolTemplate {
    /// Exactly one of smiles / cxsmiles / molfile must be set.
    pub fn validate_structure(&self) -> Result<(), String> {
        let n = [&self.smiles, &self.cxsmiles, &self.molfile]
            .iter()
            .filter(|s| s.as_ref().is_some_and(|t| !t.trim().is_empty()))
            .count();
        if n == 1 {
            Ok(())
        } else {
            Err(format!(
                "MolTemplate {}: need exactly one of smiles/cxsmiles/molfile",
                self.id
            ))
        }
    }

    pub fn source(&self) -> Result<&str, String> {
        self.validate_structure()?;
        if let Some(s) = self.smiles.as_deref().filter(|t| !t.trim().is_empty()) {
            return Ok(s);
        }
        if let Some(s) = self.cxsmiles.as_deref().filter(|t| !t.trim().is_empty()) {
            return Ok(s);
        }
        if let Some(s) = self.molfile.as_deref().filter(|t| !t.trim().is_empty()) {
            return Ok(s);
        }
        Err(format!("MolTemplate {}: empty structure", self.id))
    }
}

/// ``type: "coord_gen"`` edge task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum EdgeTask {
    CoordGen {
        #[serde(default)]
        roots: Vec<MolTemplate>,
    },
}

/// Host callback request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct EdgePlan {
    pub version: u32,
    #[serde(default)]
    pub tasks: Vec<EdgeTask>,
}

impl EdgePlan {
    pub fn new_v1(tasks: Vec<EdgeTask>) -> Self {
        Self {
            version: 1,
            tasks,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("unsupported EdgePlan version {}", self.version));
        }
        let mut seen = std::collections::HashSet::new();
        for task in &self.tasks {
            match task {
                EdgeTask::CoordGen { roots } => {
                    for root in roots {
                        validate_tree(root, /*is_root*/ true, &mut seen)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_tree(
    node: &MolTemplate,
    is_root: bool,
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    if !seen.insert(node.id.clone()) {
        return Err(format!("duplicate MolTemplate id {}", node.id));
    }
    node.validate_structure()?;
    if is_root && node.align.is_some() {
        return Err(format!(
            "MolTemplate {}: roots must have align=null",
            node.id
        ));
    }
    if !is_root && node.align.is_none() {
        // Children may omit align object (= MCS defaults); that's OK.
    }
    for child in &node.template_for {
        validate_tree(child, false, seen)?;
    }
    Ok(())
}

/// How coords were produced for one molecule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum CoordMethod {
    Free,
    AtomMap,
    Mcs,
    /// Align was requested but failed — host fell back to unaligned coord gen.
    None,
}

/// One flat molecule entry in a coord_gen result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct CoordGenMoleculeResult {
    pub id: String,
    /// True when usable coords were produced (aligned or free-layout fallback).
    pub ok: bool,
    pub method: CoordMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub used_map: Option<Vec<(u32, u32)>>,
    /// Present whenever ``ok`` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub molecule: Option<MoleculeIn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "codegen", ts(optional))]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub enum EdgeTaskResult {
    CoordGen {
        ok: bool,
        molecules: Vec<CoordGenMoleculeResult>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(JsonSchema, TS))]
#[cfg_attr(feature = "codegen", ts(export))]
pub struct EdgeResult {
    pub version: u32,
    #[serde(default)]
    pub results: Vec<EdgeTaskResult>,
}

impl EdgeResult {
    pub fn new_v1(results: Vec<EdgeTaskResult>) -> Self {
        Self {
            version: 1,
            results,
        }
    }

    /// Collect all molecule rows across tasks (order preserved).
    pub fn molecules(&self) -> Vec<&CoordGenMoleculeResult> {
        let mut out = Vec::new();
        for r in &self.results {
            match r {
                EdgeTaskResult::CoordGen { molecules, .. } => out.extend(molecules.iter()),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> EdgePlan {
        EdgePlan::new_v1(vec![EdgeTask::CoordGen {
            roots: vec![MolTemplate {
                id: "m_0".into(),
                smiles: Some("c1ccccc1".into()),
                cxsmiles: None,
                molfile: None,
                align: None,
                template_for: vec![MolTemplate {
                    id: "m_1".into(),
                    smiles: Some("Cc1ccccc1".into()),
                    cxsmiles: None,
                    molfile: None,
                    align: Some(AlignOpts {
                        atom_map: Some(vec![
                            (1, 0),
                            (2, 1),
                            (3, 2),
                            (4, 3),
                            (5, 4),
                            (6, 5),
                        ]),
                        min_atoms: None,
                    }),
                    template_for: vec![],
                }],
            }],
        }])
    }

    #[test]
    fn plan_json_roundtrip() {
        let plan = sample_plan();
        plan.validate().unwrap();
        let s = serde_json::to_string(&plan).unwrap();
        let back: EdgePlan = serde_json::from_str(&s).unwrap();
        assert_eq!(back, plan);
    }

    #[test]
    fn reject_duplicate_ids() {
        let mut plan = sample_plan();
        match &mut plan.tasks[0] {
            EdgeTask::CoordGen { roots } => {
                roots[0].template_for[0].id = "m_0".into();
            }
        }
        assert!(plan.validate().unwrap_err().contains("duplicate"));
    }

    #[test]
    fn reject_root_with_align() {
        let plan = EdgePlan::new_v1(vec![EdgeTask::CoordGen {
            roots: vec![MolTemplate {
                id: "m_0".into(),
                smiles: Some("CCO".into()),
                cxsmiles: None,
                molfile: None,
                align: Some(AlignOpts::default()),
                template_for: vec![],
            }],
        }]);
        assert!(plan.validate().unwrap_err().contains("align=null"));
    }

    #[test]
    fn fallback_result_lists_all_ids() {
        let result = EdgeResult::new_v1(vec![EdgeTaskResult::CoordGen {
            ok: true,
            molecules: vec![
                CoordGenMoleculeResult {
                    id: "m_0".into(),
                    ok: true,
                    method: CoordMethod::Free,
                    used_map: None,
                    molecule: None,
                    error: None,
                },
                CoordGenMoleculeResult {
                    id: "m_1".into(),
                    ok: true,
                    method: CoordMethod::None,
                    used_map: None,
                    molecule: None,
                    error: Some("align failed; fell back".into()),
                },
            ],
        }]);
        let ids: Vec<_> = result.molecules().iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["m_0", "m_1"]);
        assert!(matches!(
            &result.results[0],
            EdgeTaskResult::CoordGen { ok: true, .. }
        ));
    }
}

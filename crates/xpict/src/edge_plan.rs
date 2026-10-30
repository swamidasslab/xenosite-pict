//! Align-plan helper + core [`EdgePlan`] processor (SMILES/molfile + CoordGen).

use std::collections::HashMap;

use xpict_core::edge::{AlignOpts, EdgePlan, EdgeResult, EdgeTask, MolTemplate};

use crate::Error;

/// Build a two-node align forest (template root + query child).
pub fn build_align_plan(
    template_source: &str,
    query_source: &str,
    atom_map: Option<Vec<(u32, u32)>>,
) -> EdgePlan {
    EdgePlan::new_v1(vec![EdgeTask::CoordGen {
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
    }])
}

/// Run all ``coord_gen`` tasks; always return flat molecule rows.
pub fn process_edge_plan(plan: &EdgePlan) -> Result<EdgeResult, Error> {
    xpict_core::process_edge_plan(plan).map_err(Error::Message)
}

/// Like [`process_edge_plan`], plus pose molblocks keyed by mol id.
pub fn process_edge_plan_with_frames(
    plan: &EdgePlan,
) -> Result<(EdgeResult, HashMap<String, String>), Error> {
    xpict_core::process_edge_plan_with_frames(plan).map_err(Error::Message)
}

//! Multi-molecule diagram placement via **elkrs** (native ELK).
//!
//! Replaces the Python `jsrun` + vendored elkjs path for network/reaction
//! diagrams. Input/output is ELK JSON — same graph the Python synthesizer
//! already builds (`elk_graph` in `diagram/elk.py`).

#![cfg(feature = "elk")]

use std::collections::BTreeMap;

use crate::doc::{EdgeRouting, LayoutAlgorithm, LayoutDirection, LayoutOpts};

/// Whether the diagram uses reaction packing defaults (vs generic network).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Network,
    Reaction,
}

fn direction_elk(d: LayoutDirection) -> &'static str {
    match d {
        LayoutDirection::Right => "RIGHT",
        LayoutDirection::Left => "LEFT",
        LayoutDirection::Up => "UP",
        LayoutDirection::Down => "DOWN",
    }
}

fn routing_elk(r: EdgeRouting) -> &'static str {
    match r {
        EdgeRouting::Orthogonal => "ORTHOGONAL",
        EdgeRouting::Polyline => "POLYLINE",
        EdgeRouting::Splines => "SPLINES",
    }
}

fn algorithm_elk(a: LayoutAlgorithm) -> &'static str {
    match a {
        LayoutAlgorithm::Layered => "layered",
        LayoutAlgorithm::Radial => "radial",
        LayoutAlgorithm::Force => "force",
        LayoutAlgorithm::Stress => "stress",
    }
}

/// Backend-agnostic [`LayoutOpts`] → ELK `layoutOptions` map.
///
/// Reaction diagrams pack tightly (node spacing ~20, layer spacing ~0) and pin
/// `unnecessaryBendpoints=false` so shafts only bend where they turn.
pub fn scheme_layout_options(
    kind: DiagramKind,
    opts: &LayoutOpts,
) -> BTreeMap<String, String> {
    let mut base: BTreeMap<String, String> = BTreeMap::from([
        ("elk.algorithm".into(), "layered".into()),
        ("elk.direction".into(), "RIGHT".into()),
        ("elk.edgeRouting".into(), "POLYLINE".into()),
        // Keep shafts simple: only bend where the edge actually turns.
        ("elk.layered.unnecessaryBendpoints".into(), "false".into()),
        ("elk.spacing.nodeNode".into(), "40".into()),
        ("elk.spacing.edgeEdge".into(), "16".into()),
        ("elk.spacing.edgeNode".into(), "20".into()),
        ("elk.layered.spacing.nodeNodeBetweenLayers".into(), "48".into()),
        ("elk.layered.spacing.edgeNodeBetweenLayers".into(), "24".into()),
        (
            "elk.layered.crossingMinimization.strategy".into(),
            "LAYER_SWEEP".into(),
        ),
        (
            "elk.layered.nodePlacement.strategy".into(),
            "NETWORK_SIMPLEX".into(),
        ),
    ]);

    if kind == DiagramKind::Reaction {
        base.insert(
            "elk.spacing.nodeNode".into(),
            LayoutOpts::DEFAULT_NODE_SPACING.to_string(),
        );
        base.insert(
            "elk.layered.spacing.nodeNodeBetweenLayers".into(),
            LayoutOpts::DEFAULT_LAYER_SPACING.to_string(),
        );
        base.insert(
            "elk.layered.spacing.edgeNodeBetweenLayers".into(),
            LayoutOpts::DEFAULT_LAYER_SPACING.to_string(),
        );
        base.insert("elk.spacing.edgeEdge".into(), "16".into());
        base.insert(
            "elk.layered.crossingMinimization.forceNodeModelOrder".into(),
            "false".into(),
        );
    }

    if opts.algorithm.is_some() {
        base.insert(
            "elk.algorithm".into(),
            algorithm_elk(opts.algorithm_or_default()).into(),
        );
    }
    if opts.direction.is_some() {
        base.insert(
            "elk.direction".into(),
            direction_elk(opts.direction_or_default()).into(),
        );
    }
    if opts.edge_routing.is_some() {
        base.insert(
            "elk.edgeRouting".into(),
            routing_elk(opts.edge_routing_or_default()).into(),
        );
    }
    if let Some(ns) = opts.node_spacing {
        base.insert("elk.spacing.nodeNode".into(), ns.to_string());
    }
    if let Some(ls) = opts.layer_spacing {
        base.insert(
            "elk.layered.spacing.nodeNodeBetweenLayers".into(),
            ls.to_string(),
        );
        base.insert(
            "elk.layered.spacing.edgeNodeBetweenLayers".into(),
            (ls * 0.5).max(0.0).to_string(),
        );
    }
    base
}

/// Lay out an ELK JSON graph string; returns laid-out JSON.
///
/// Errors are returned as strings (invalid JSON, unknown algorithm, …).
pub fn layout_json(input: &str) -> Result<String, String> {
    let value = elkrs::create_elk().layout_json(input)?;
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_defaults_tight_pack_no_extra_bends() {
        let opts = LayoutOpts::default();
        let m = scheme_layout_options(DiagramKind::Reaction, &opts);
        assert_eq!(m.get("elk.algorithm").unwrap(), "layered");
        assert_eq!(m.get("elk.layered.unnecessaryBendpoints").unwrap(), "false");
        assert_eq!(
            m.get("elk.spacing.nodeNode").unwrap(),
            &LayoutOpts::DEFAULT_NODE_SPACING.to_string()
        );
        assert_eq!(
            m.get("elk.layered.spacing.nodeNodeBetweenLayers").unwrap(),
            &LayoutOpts::DEFAULT_LAYER_SPACING.to_string()
        );
    }

    #[test]
    fn algorithm_override_selects_force() {
        let opts = LayoutOpts {
            algorithm: Some(LayoutAlgorithm::Force),
            ..Default::default()
        };
        let m = scheme_layout_options(DiagramKind::Network, &opts);
        assert_eq!(m.get("elk.algorithm").unwrap(), "force");
    }

    #[test]
    fn layered_orthogonal_emits_edge_sections() {
        let input = r#"{
          "id": "root",
          "layoutOptions": {
            "elk.algorithm": "layered",
            "elk.direction": "RIGHT",
            "elk.edgeRouting": "ORTHOGONAL"
          },
          "children": [
            {"id": "a", "width": 40.0, "height": 30.0},
            {"id": "extra", "width": 10.0, "height": 10.0},
            {"id": "b", "width": 40.0, "height": 30.0}
          ],
          "edges": [{"id": "e0", "sources": ["a"], "targets": ["b"]}]
        }"#;
        let out = layout_json(input).expect("layout");
        assert!(out.contains("\"x\""), "{out}");
        assert!(
            out.contains("sections") && out.contains("startPoint"),
            "{out}"
        );
        // Parity with Python ``test_elk_layout_json_layered``: b is right of a.
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
        let children = parsed["children"].as_array().expect("children");
        let mut ax = None;
        let mut bx = None;
        for c in children {
            match c["id"].as_str() {
                Some("a") => ax = c["x"].as_f64(),
                Some("b") => bx = c["x"].as_f64(),
                _ => {}
            }
        }
        assert!(bx.unwrap() > ax.unwrap(), "b.x should be > a.x: {out}");
        let edge = &parsed["edges"][0];
        assert!(edge["sections"].as_array().map(|s| !s.is_empty()).unwrap_or(false));
        assert!(edge["sections"][0].get("startPoint").is_some());
    }

    #[test]
    fn layered_ignores_extra_children_in_assert_loop() {
        // Extra node exercises the `_` arm when scanning children by id.
        let input = r#"{
          "id": "root",
          "layoutOptions": {
            "elk.algorithm": "layered",
            "elk.direction": "RIGHT",
            "elk.edgeRouting": "ORTHOGONAL"
          },
          "children": [
            {"id": "a", "width": 40.0, "height": 30.0},
            {"id": "extra", "width": 20.0, "height": 20.0},
            {"id": "b", "width": 40.0, "height": 30.0}
          ],
          "edges": [{"id": "e0", "sources": ["a"], "targets": ["b"]}]
        }"#;
        let out = layout_json(input).expect("layout");
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert!(parsed["children"].as_array().unwrap().len() >= 3);
    }
}

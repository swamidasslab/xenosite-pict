//! Multi-molecule diagram placement via **elkrs** (native ELK).
//!
//! Replaces the Python `jsrun` + vendored elkjs path for network/reaction
//! diagrams. Input/output is ELK JSON — same graph the Python synthesizer
//! already builds (`elk_graph` in `diagram/elk.py`).

#![cfg(feature = "elk")]

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

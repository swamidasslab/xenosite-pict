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
        let a_x = out.find(r#""id":"a""#).expect("node a");
        let b_x = out.find(r#""id":"b""#).expect("node b");
        assert_ne!(a_x, b_x);
        // Layered RIGHT should put b to the right of a (different x).
        assert!(out.contains(r#""width":124.0"#) || out.contains(r#""width":124"#), "{out}");
    }
}

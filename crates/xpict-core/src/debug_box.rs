//! Debug bounding-box overlays for layout / margin inspection.
//!
//! Not for publication figures — a diagnostic aid for boxes and margins.
//! When [`crate::scene::MoleculeIn::debug_box`] (or cascading opts) is on,
//! depict emits stroked rectangles for label ink bounds and the mol canvas.
//! Compose adds a document-level box around the full scene. Atom/bond stroke
//! primitives are never wrapped. Strokes use partial opacity so stacked
//! boxes remain distinguishable where they overlap.

use crate::scene::Primitive;

/// Stroke color for debug rectangles (high-contrast, not chem ink).
pub const DEBUG_BOX_STROKE: &str = "#e91e63";
/// Hairline in house SCALE units (scales with diagram scale).
pub const DEBUG_BOX_STROKE_PX: f64 = 1.0;
pub const DEBUG_BOX_DASH: &str = "4 2";
/// Partial alpha so overlapping label / mol / scene borders stay readable.
pub const DEBUG_BOX_OPACITY: f64 = 0.45;

/// Axis-aligned debug rectangle as a closed path primitive.
pub fn debug_rect(x: f64, y: f64, w: f64, h: f64, class: &str) -> Primitive {
    let x2 = x + w;
    let y2 = y + h;
    Primitive::Path {
        d: format!(
            "M {:.2} {:.2} H {:.2} V {:.2} H {:.2} Z",
            x, y, x2, y2, x
        ),
        stroke: Some(DEBUG_BOX_STROKE.into()),
        fill: Some("none".into()),
        stroke_width: DEBUG_BOX_STROKE_PX,
        opacity: DEBUG_BOX_OPACITY,
        stroke_dasharray: Some(DEBUG_BOX_DASH.into()),
        stroke_linecap: Some("butt".into()),
        class: Some(class.into()),
        data_text: None,
    }
}

/// Label ink AABB → debug rect (`debug-box debug-box-label …`).
pub fn label_debug_rect(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    atom_index: i32,
) -> Primitive {
    debug_rect(
        min_x,
        min_y,
        (max_x - min_x).max(0.0),
        (max_y - min_y).max(0.0),
        &format!("debug-box debug-box-label atom-{atom_index}"),
    )
}

/// Mol viewport / canvas box.
pub fn mol_debug_rect(width: f64, height: f64) -> Primitive {
    debug_rect(0.0, 0.0, width, height, "debug-box debug-box-mol")
}

/// Full scene / document box.
pub fn scene_debug_rect(width: f64, height: f64) -> Primitive {
    debug_rect(0.0, 0.0, width, height, "debug-box debug-box-scene")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_rect_is_closed_path() {
        let p = debug_rect(1.0, 2.0, 10.0, 20.0, "debug-box");
        match p {
            Primitive::Path {
                d,
                fill: Some(f),
                stroke: Some(s),
                opacity,
                class: Some(c),
                ..
            } => {
                assert!(d.ends_with('Z') || d.ends_with("Z"));
                assert_eq!(f, "none");
                assert_eq!(s, DEBUG_BOX_STROKE);
                assert!(
                    (opacity - DEBUG_BOX_OPACITY).abs() < 1e-9 && opacity < 1.0,
                    "debug borders need alpha so overlaps are visible; got {opacity}"
                );
                assert!(c.contains("debug-box"));
            }
            _ => panic!("expected path"),
        }
    }
}

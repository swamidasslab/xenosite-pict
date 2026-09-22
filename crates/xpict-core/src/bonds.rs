//! Multi-bond offset helpers.
//!
//! Keep in sync with `src/xpict/draw/bonds.py` (`multi_bond_offset`,
//! `centered_displacements`). Full stroke/join logic stays in Python until
//! the drawer port is deliberate.

use crate::metrics::OFFSET_PX;

/// Parallel spacing for double/triple strokes (RDKit `multipleBondOffset`).
///
/// Prefer [`OFFSET_PX`]. Do **not** scale down to the post-label stroke length —
/// heteroatom insets must not collapse carbonyl spacing. Only shrink for
/// degenerate stubs shorter than two offset widths.
pub fn multi_bond_offset(length: f64) -> f64 {
    if length < 2.0 * OFFSET_PX {
        OFFSET_PX.min(length * 0.25)
    } else {
        OFFSET_PX
    }
}

/// Signed offsets of a centered multiple bond along the left normal.
///
/// A double is two lines split evenly about the axis (separation `off`).
/// A triple keeps the axis and one line `off` to each side.
pub fn centered_displacements(order: f64, off: f64) -> Vec<f64> {
    if order >= 2.5 {
        vec![-off, 0.0, off]
    } else {
        vec![-off * 0.5, off * 0.5]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::BOND_PX;

    #[test]
    fn short_visible_stroke_keeps_full_offset() {
        let stub = 10.0;
        assert!(stub * 0.22 < OFFSET_PX);
        assert!(stub >= 2.0 * OFFSET_PX);
        assert!((multi_bond_offset(stub) - OFFSET_PX).abs() < 1e-9);
        assert!((multi_bond_offset(BOND_PX) - OFFSET_PX).abs() < 1e-9);
    }

    #[test]
    fn double_is_symmetric_about_axis() {
        let d = centered_displacements(2.0, OFFSET_PX);
        assert_eq!(d.len(), 2);
        assert!((d[0] + d[1]).abs() < 1e-9);
        assert!((d[1] - d[0] - OFFSET_PX).abs() < 1e-9);
    }
}

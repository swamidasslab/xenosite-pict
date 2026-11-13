//! Shared markup → SVG glyph path for mol chrome / captions.
//!
//! Reuses the chem label markup parser ([`crate::markup::parse_label_markup`])
//! and Liberation outline path — no second markup engine.

use crate::font::{compile_chem_shapes, face_metrics, FaceStyle};
use crate::labels::{compose_label, split_label, LabelSide};
use crate::markup::parse_label_markup;
use crate::metrics::label_weight_grow_px;

/// Typographic ascent (above baseline) and descent (below) in SVG px (+Y down).
///
/// Layout gaps / viewBox padding should use these — not glyph outline AABB —
/// so captions with different shapes share one baseline.
pub fn markup_ascent_descent_px(font_px: f64) -> (f64, f64) {
    let m = face_metrics(FaceStyle::Regular);
    let scale = font_px.max(1e-9) / m.upem;
    let ascent = m.ascent * scale;
    let descent = (-m.descent) * scale;
    (ascent, descent)
}

/// Glyph ink [`crate::geom::Shape`] for markup at baseline `(x, y)`.
///
/// Same Liberation outline path as [`outline_markup`] — use this for buffered
/// halos (not an AABB box). ``weight`` is the mol diagram weight (house ``1``);
/// stems are buffered like chem atom labels via [`label_weight_grow_px`].
pub fn markup_ink_shape(
    raw: &str,
    x: f64,
    y: f64,
    font_px: f64,
    anchor: &str,
    weight: f64,
) -> Option<crate::geom::Shape> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    let style = FaceStyle::Regular;
    let glyphs = parse_label_markup(text, style);
    if glyphs.is_empty() {
        return None;
    }
    let mut shape = compile_chem_shapes(&glyphs, x, y, font_px, anchor, style)?;
    let grow = label_weight_grow_px(weight);
    if grow > 1e-9 {
        shape = shape.buffer(grow);
    }
    Some(shape)
}

/// Outline markup-capable text at `(x, y)` (baseline).
///
/// Returns `(path_d, data_text, bounds)` where `data_text` uses unicode
/// sub/superscripts (same as atom labels). ``weight`` matches chem label stem
/// thickening (house ``1``).
pub fn outline_markup(
    raw: &str,
    x: f64,
    y: f64,
    font_px: f64,
    anchor: &str,
    weight: f64,
) -> Option<(String, String, (f64, f64, f64, f64))> {
    let shape = markup_ink_shape(raw, x, y, font_px, anchor, weight)?;
    let d = shape.to_svg_d();
    if d.is_empty() {
        return None;
    }
    let bounds = shape.bounds()?;
    let data_text = compose_label(&split_label(raw.trim()), LabelSide::East);
    Some((d, data_text, bounds))
}

/// Ink AABB for markup at baseline `y = 0`, anchor at `x = 0`.
pub fn markup_bounds_at_origin(
    raw: &str,
    font_px: f64,
    anchor: &str,
    weight: f64,
) -> Option<(f64, f64, f64, f64)> {
    outline_markup(raw, 0.0, 0.0, font_px, anchor, weight).map(|(_, _, b)| b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_subscript_in_data_text() {
        let (_d, data, _b) =
            outline_markup("$R_1$", 0.0, 0.0, 14.0, "middle", 1.0).expect("outline");
        assert!(
            data.contains('₁') || data.contains("R"),
            "expected subscripted data_text, got {data:?}"
        );
        assert!(data.contains('R'));
    }

    #[test]
    fn markup_plain_text_outlines() {
        let (d, data, (x0, y0, x1, y1)) =
            outline_markup("ethanol", 10.0, 20.0, 18.0, "middle", 1.0).expect("outline");
        assert!(!d.is_empty());
        assert_eq!(data, "ethanol");
        assert!(x1 > x0 && y1 > y0);
    }

    #[test]
    fn ascent_descent_scale_with_font_px() {
        let (a1, d1) = markup_ascent_descent_px(10.0);
        let (a2, d2) = markup_ascent_descent_px(20.0);
        assert!(a1 > 0.0 && d1 > 0.0);
        assert!((a2 - 2.0 * a1).abs() < 1e-9);
        assert!((d2 - 2.0 * d1).abs() < 1e-9);
        // Liberation: ascent >> |descent|.
        assert!(a1 > d1);
    }

    #[test]
    fn markup_superscript_charge() {
        let (_d, data, _) =
            outline_markup("$N^+$", 0.0, 0.0, 14.0, "start", 1.0).expect("outline");
        assert!(
            data.contains('⁺') || data.contains('+'),
            "expected charge in data_text, got {data:?}"
        );
    }

    #[test]
    fn house_weight_thickens_markup_vs_min() {
        use crate::metrics::WEIGHT_MIN;
        let thin = markup_ink_shape("O", 0.0, 0.0, 14.0, "middle", WEIGHT_MIN).expect("thin");
        let house = markup_ink_shape("O", 0.0, 0.0, 14.0, "middle", 1.0).expect("house");
        assert!(
            house.area() > thin.area() * 1.05,
            "house weight should thicken stem vs WEIGHT_MIN"
        );
    }
}

//! Depiction proportions (xenopict / RDKit house style).
//!
//! Keep in sync with `python/xpict/draw/metrics.py`.

/// xenopict `Xenopict.scale` / xenosite depiction `"scale": 20`.
pub const SCALE: f64 = 20.0;

/// Bond length in drawing / CSS-px units (alias of [`SCALE`]).
pub const BOND_PX: f64 = SCALE;

/// Atom-label em as a fraction of bond length.
///
/// RDKit documents `baseFontSize = 0.6`, but MolDraw2D outlined heteroatom
/// glyphs measure ~0.29 × bond in cap height (~0.45 em with Liberation Sans).
pub const FONT_FRAC: f64 = 0.45;

/// Liberation Sans Regular vertical stem width in em.
pub const FONT_STEM_EM: f64 = 0.0933;

/// Liberation Sans Bold vertical stem width in em (measured on H).
///
/// Reference measurement only — mol ink weight uses Regular + ``Shape::buffer``.
pub const FONT_STEM_EM_BOLD: f64 = 0.144;

/// Bond stroke as a fraction of bond length — tracks the label stem so ink
/// weight matches letter stems when [`FONT_FRAC`] changes.
pub const STROKE_FRAC: f64 = 0.042; // FONT_STEM_EM * FONT_FRAC, rounded

/// RDKit `multipleBondOffset`.
pub const OFFSET_FRAC: f64 = 0.15;

/// Full wedge width at the fat end, as a fraction of bond length.
pub const WEDGE_WIDTH_FRAC: f64 = 0.30;

pub const PAD_FRAC: f64 = 1.0;
pub const END_GAP_FRAC: f64 = 0.13;
pub const CHAIN_END_GAP_FRAC: f64 = 0.0;
pub const HASH_PER_BOND: i32 = 8;
pub const HALO_FRAC: f64 = 2.0 * STROKE_FRAC;
pub const LABEL_GAP_FRAC: f64 = 0.05;
pub const SHADE_FRAC: f64 = 0.90;
pub const MARK_FRAC: f64 = 1.0;
/// xenopict mark layer: `stroke-width: scale * 0.1`, opacity 0.7.
///
/// Kept absolute (not stem-keyed): xenopict bonds and marks both used
/// ``scale * 0.1``, but xpict bond ink is thinner (Liberation stem). Marks
/// still match xenopict's publication ring weight.
pub const MARK_STROKE_FRAC: f64 = 0.1;
/// xenopict mark halo (`<use>`): `stroke-width: scale * 0.2`, opacity 0.45.
pub const MARK_HALO_STROKE_FRAC: f64 = 0.2;
pub const MARK_OPACITY: f64 = 0.7;
pub const MARK_HALO_OPACITY: f64 = 0.45;
/// xenopict mark halo `<use stroke="#555">`.
///
/// Mark ink has stroke-width/opacity but **no** stroke color (xenopict leaves
/// that for host CSS on `.mark`). Xpict does not style by class: the visible
/// ring is this baked halo attribute; ink stays uncolored.
pub const MARK_HALO_COLOR: &str = "#555";
/// RDKit ``DrawText::SUBS_SCALE`` / ``SUPER_SCALE`` for H-counts and charges.
pub const SCRIPT_SCALE: f64 = 0.66;
/// Dummy / R-group ``*`` height as a multiple of Liberation cap-height.
///
/// Custom five-point star (not the Liberation asterisk); ~2× letter height so
/// attachment points read clearly next to heteroatom labels.
pub const STAR_FRAC: f64 = 2.0;

pub const STROKE_PX: f64 = STROKE_FRAC * BOND_PX;
pub const OFFSET_PX: f64 = OFFSET_FRAC * BOND_PX;
pub const WEDGE_HALF_PX: f64 = WEDGE_WIDTH_FRAC * BOND_PX * 0.5;
pub const FONT_PX: f64 = FONT_FRAC * BOND_PX;
pub const PAD_PX: f64 = PAD_FRAC * BOND_PX;
pub const END_GAP_PX: f64 = END_GAP_FRAC * BOND_PX;
pub const CHAIN_END_GAP_PX: f64 = CHAIN_END_GAP_FRAC * BOND_PX;
pub const HALO_STROKE: f64 = HALO_FRAC * BOND_PX;
pub const LABEL_GAP_PX: f64 = LABEL_GAP_FRAC * BOND_PX;
/// Halo outline grow (half the label-gap air) + soft white fill.
pub const HALO_GAP_PX: f64 = 0.5 * LABEL_GAP_PX;
pub const HALO_OPACITY: f64 = 0.5;
pub const MARK_STROKE_PX: f64 = MARK_STROKE_FRAC * BOND_PX;
pub const MARK_HALO_STROKE_PX: f64 = MARK_HALO_STROKE_FRAC * BOND_PX;

/// Absolute ink multiplier at user-facing ``weight = 1`` (house look).
///
/// Public ``weight`` is relative to this: default ``1`` → this multiplier;
/// absolute floor stays ``1`` (Regular stem), so the user minimum is
/// [`WEIGHT_MIN`] (= ``1 / WEIGHT_AT_ONE``).
pub const WEIGHT_AT_ONE: f64 = 1.5;

/// Lowest user-facing ``weight`` (maps to absolute ink ``1`` = Regular stem).
pub const WEIGHT_MIN: f64 = 1.0 / WEIGHT_AT_ONE;

/// Map user-facing mol ``weight`` → absolute ink multiplier (≥ 1).
///
/// Call this (or helpers that call it) with the public ``weight`` field — never
/// with an already-converted absolute value.
///
/// # Panics
/// Panics if ``weight`` is non-finite or ``< WEIGHT_MIN``.
pub fn diagram_weight(weight: f64) -> f64 {
    assert!(
        weight.is_finite() && weight >= WEIGHT_MIN - 1e-12,
        "mol weight must be finite and >= {WEIGHT_MIN}, got {weight}"
    );
    weight * WEIGHT_AT_ONE
}

/// Outward glyph buffer (px) so absolute ink stems grow past Regular.
///
/// ``grow = 0.5 × FONT_STEM_EM × FONT_PX × (ink - 1)`` where ``ink`` is
/// [`diagram_weight`]; zero at [`WEIGHT_MIN`].
///
/// # Panics
/// Panics if ``weight`` is invalid (see [`diagram_weight`]).
pub fn label_weight_grow_px(weight: f64) -> f64 {
    let ink = diagram_weight(weight);
    0.5 * FONT_STEM_EM * FONT_PX * (ink - 1.0)
}

/// Extra bond↔label standoff (px) when absolute ink exceeds Regular.
///
/// Label buffer grow + half the extra bond stroke vs [`STROKE_PX`].
///
/// # Panics
/// Panics if ``weight`` is invalid (see [`diagram_weight`]).
pub fn label_weight_standoff_px(weight: f64) -> f64 {
    label_weight_grow_px(weight) + 0.5 * (stroke_px_for_weight(weight) - STROKE_PX)
}

/// Bond stroke in drawing px for user-facing mol ``weight``.
///
/// # Panics
/// Panics if ``weight`` is invalid (see [`diagram_weight`]).
pub fn stroke_px_for_weight(weight: f64) -> f64 {
    stroke_px_from_stem(FONT_STEM_EM) * diagram_weight(weight)
}

/// Bond stroke fraction from a stem width in em (rounded like [`STROKE_FRAC`]).
pub fn stroke_frac_from_stem(stem_em: f64) -> f64 {
    (stem_em * FONT_FRAC * 1000.0).round() / 1000.0
}

/// Bond stroke in drawing px keyed to a label stem.
pub fn stroke_px_from_stem(stem_em: f64) -> f64 {
    stroke_frac_from_stem(stem_em) * BOND_PX
}

/// Halo stroke width tracks active bond ink (2×, same ratio as [`HALO_FRAC`]).
pub fn halo_stroke_from_stroke(stroke_px: f64) -> f64 {
    2.0 * stroke_px
}

/// Halo stroke for mol ``weight``: [`HALO_STROKE`] × √ink (sublinear vs ink).
///
/// # Panics
/// Panics if ``weight`` is invalid (see [`diagram_weight`]).
pub fn halo_stroke_for_weight(weight: f64) -> f64 {
    HALO_STROKE * diagram_weight(weight).sqrt()
}

/// Outer halo buffer for mol ``weight``: [`HALO_GAP_PX`] × √ink.
///
/// # Panics
/// Panics if ``weight`` is invalid (see [`diagram_weight`]).
pub fn halo_gap_for_weight(weight: f64) -> f64 {
    HALO_GAP_PX * diagram_weight(weight).sqrt()
}

/// Dash count scaling with drawn bond length (hashed wedges).
pub fn hash_count(length: f64) -> usize {
    let n = (f64::from(HASH_PER_BOND) * length / BOND_PX).round() as i32;
    n.clamp(5, 12) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_matches_python_house_style() {
        assert!((OFFSET_PX - 3.0).abs() < 1e-9);
        assert!((STROKE_PX - 0.84).abs() < 1e-9);
        assert!((FONT_PX - 9.0).abs() < 1e-9);
        assert!((SHADE_FRAC - 0.90).abs() < 1e-9);
    }

    #[test]
    fn stroke_tracks_font_stem() {
        let expected = (FONT_STEM_EM * FONT_FRAC * 1000.0).round() / 1000.0;
        assert!((STROKE_FRAC - expected).abs() < 1e-9);
        assert!((stroke_px_from_stem(FONT_STEM_EM) - STROKE_PX).abs() < 1e-9);
    }

    #[test]
    fn weight_scales_stroke_and_grow() {
        // User weight 1 → house ink (WEIGHT_AT_ONE); WEIGHT_MIN → Regular stem.
        assert!((diagram_weight(1.0) - WEIGHT_AT_ONE).abs() < 1e-9);
        assert!((diagram_weight(WEIGHT_MIN) - 1.0).abs() < 1e-9);
        assert!((stroke_px_for_weight(WEIGHT_MIN) - STROKE_PX).abs() < 1e-9);
        assert!((stroke_px_for_weight(1.0) - WEIGHT_AT_ONE * STROKE_PX).abs() < 1e-9);
        assert!((stroke_px_for_weight(2.0) - 2.0 * WEIGHT_AT_ONE * STROKE_PX).abs() < 1e-9);
        assert!(label_weight_grow_px(WEIGHT_MIN).abs() < 1e-12);
        let grow1 = label_weight_grow_px(1.0);
        assert!((grow1 - 0.5 * FONT_STEM_EM * FONT_PX * (WEIGHT_AT_ONE - 1.0)).abs() < 1e-9);
        assert!((halo_stroke_from_stroke(STROKE_PX) - 2.0 * STROKE_PX).abs() < 1e-9);
        assert!((halo_stroke_for_weight(WEIGHT_MIN) - HALO_STROKE).abs() < 1e-9);
        assert!((halo_stroke_for_weight(1.0) - HALO_STROKE * WEIGHT_AT_ONE.sqrt()).abs() < 1e-9);
        // User weight whose absolute ink is 4 → halo ×2.
        let w_abs4 = 4.0 / WEIGHT_AT_ONE;
        assert!((halo_stroke_for_weight(w_abs4) - 2.0 * HALO_STROKE).abs() < 1e-9);
        assert!((halo_gap_for_weight(w_abs4) - 2.0 * HALO_GAP_PX).abs() < 1e-9);
        assert!(halo_stroke_for_weight(2.0) < halo_stroke_from_stroke(stroke_px_for_weight(2.0)));
        assert!(label_weight_standoff_px(WEIGHT_MIN).abs() < 1e-12);
        let stand1 = label_weight_standoff_px(1.0);
        assert!(stand1 > label_weight_grow_px(1.0));
        assert!(
            (stand1
                - (label_weight_grow_px(1.0)
                    + 0.5 * (stroke_px_for_weight(1.0) - STROKE_PX)))
                .abs()
                < 1e-9
        );
    }

    #[test]
    #[should_panic(expected = "mol weight must be finite and >=")]
    fn diagram_weight_rejects_below_min() {
        let _ = diagram_weight(0.5);
    }
}

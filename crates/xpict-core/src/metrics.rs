//! Depiction proportions (xenopict / RDKit house style).
//!
//! Keep in sync with `src/xpict/draw/metrics.py`.

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
pub const MARK_STROKE_FRAC: f64 = 0.1;
/// xenopict mark halo (`<use>`): `stroke-width: scale * 0.2`, opacity 0.45.
pub const MARK_HALO_STROKE_FRAC: f64 = 0.2;
pub const MARK_OPACITY: f64 = 0.7;
pub const MARK_HALO_OPACITY: f64 = 0.45;
/// RDKit ``DrawText::SUBS_SCALE`` / ``SUPER_SCALE`` for H-counts and charges.
pub const SCRIPT_SCALE: f64 = 0.66;
/// Dummy / R-group ``*`` drawn larger than a Liberation asterisk.
pub const STAR_FRAC: f64 = 1.35;

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
    }
}

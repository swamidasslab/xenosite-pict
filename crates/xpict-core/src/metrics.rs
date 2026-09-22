//! Depiction proportions (xenopict / RDKit house style).
//!
//! Keep in sync with `src/xpict/draw/metrics.py`.

/// xenopict `Xenopict.scale` — RDKit `fixedBondLength`.
pub const BOND_PX: f64 = 20.0;

/// RDKit `baseFontSize`.
pub const FONT_FRAC: f64 = 0.60;

/// Liberation Sans Regular vertical stem width in em.
pub const FONT_STEM_EM: f64 = 0.0933;

/// Bond stroke as a fraction of bond length (font stem, not xenopict's 0.10).
pub const STROKE_FRAC: f64 = 0.056; // FONT_STEM_EM * FONT_FRAC, rounded

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

pub const STROKE_PX: f64 = STROKE_FRAC * BOND_PX;
pub const OFFSET_PX: f64 = OFFSET_FRAC * BOND_PX;
pub const WEDGE_HALF_PX: f64 = WEDGE_WIDTH_FRAC * BOND_PX * 0.5;
pub const FONT_PX: f64 = FONT_FRAC * BOND_PX;
pub const PAD_PX: f64 = PAD_FRAC * BOND_PX;
pub const END_GAP_PX: f64 = END_GAP_FRAC * BOND_PX;
pub const CHAIN_END_GAP_PX: f64 = CHAIN_END_GAP_FRAC * BOND_PX;
pub const HALO_STROKE: f64 = HALO_FRAC * BOND_PX;
pub const LABEL_GAP_PX: f64 = LABEL_GAP_FRAC * BOND_PX;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_matches_python_house_style() {
        assert!((OFFSET_PX - 3.0).abs() < 1e-9);
        assert!((STROKE_PX - 1.12).abs() < 1e-9);
        assert!((SHADE_FRAC - 0.90).abs() < 1e-9);
    }
}

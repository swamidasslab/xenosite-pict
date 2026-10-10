//! Chemistry-facing 2D layout via private CoordGen backend.
//!
//! Enabled with the `coordgen` feature. Python/JS should not see CoordGen types.

pub use coordgen_sys::{
    generate_2d_coordinates, CoordGenError, LayoutMolecule, LayoutResult, BOND_STEREO_CIS,
    BOND_STEREO_NONE, BOND_STEREO_TRANS, FLAG_BEST_PRECISION, FLAG_EVEN_ANGLES,
    FLAG_FORCE_OPEN_MACROCYCLES, FLAG_FORCE_PLANAR, FLAG_MACROCYCLE_MODE, FLAG_METAL_BONDS_AS_ZOB,
    FLAG_QUICK_PRECISION, FLAG_SKIP_MINIMIZATION,
};

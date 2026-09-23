//! Local cxx bridge: RDKit Depictor + bond iteration (missing from crates.io `rdkit`).
//!
//! The public `rdkit` crate supplies SMILES/molblock parse → molblock. This bridge
//! takes that molblock, runs `compute2DCoords` /
//! `generateDepictionMatching2DStructure`, and returns atoms/bonds/coords.

#[cxx::bridge(namespace = "xpict_depict")]
pub mod ffi {
    struct LaidAtom {
        index: i32,
        z: i32,
        charge: i32,
        total_hs: u32,
        x: f64,
        y: f64,
        symbol: String,
    }

    struct LaidBond {
        index: i32,
        begin: i32,
        end: i32,
        order: f64,
        stereo: String,
    }

    struct LayoutOut {
        molblock: String,
        atoms: Vec<LaidAtom>,
        bonds: Vec<LaidBond>,
        /// True when a template was provided and MCS+Depictor matched.
        matched_template: bool,
    }

    unsafe extern "C++" {
        include!("depict_bridge.h");

        fn prepare_layout(molblock: &str, template_molblock: &str) -> Result<LayoutOut>;
        fn prepare_layout_mapped(
            molblock: &str,
            template_molblock: &str,
            atom_map_qt: &[i32],
        ) -> Result<LayoutOut>;
    }
}

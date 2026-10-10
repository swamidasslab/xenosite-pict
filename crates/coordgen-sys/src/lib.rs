//! Private CoordGen backend: C++ shim + safe Rust wrapper.
//!
//! Host crates should call [`generate_2d_coordinates`] — not CoordGen types.

use std::os::raw::c_int;
use std::ptr::null;
use thiserror::Error;

/// `sketcherMinimizer::setEvenAngles(true)`
pub const FLAG_EVEN_ANGLES: i32 = 1 << 0;
/// ctor `SKETCHER_QUICK_PRECISION`
pub const FLAG_QUICK_PRECISION: i32 = 1 << 1;
/// `sketcherMinimizer::setForceOpenMacrocycles(true)`
pub const FLAG_FORCE_OPEN_MACROCYCLES: i32 = 1 << 2;
/// ctor `SKETCHER_BEST_PRECISION`
pub const FLAG_BEST_PRECISION: i32 = 1 << 3;
/// `setTreatNonterminalBondsToMetalAsZOBs(true)`
pub const FLAG_METAL_BONDS_AS_ZOB: i32 = 1 << 4;
/// `setSkipMinimization(true)`
pub const FLAG_SKIP_MINIMIZATION: i32 = 1 << 5;

/// Draft aliases.
pub const FLAG_FORCE_PLANAR: i32 = FLAG_EVEN_ANGLES;
pub const FLAG_MACROCYCLE_MODE: i32 = FLAG_FORCE_OPEN_MACROCYCLES;

pub const BOND_STEREO_NONE: i32 = 0;
pub const BOND_STEREO_CIS: i32 = 1;
pub const BOND_STEREO_TRANS: i32 = 2;

const COORDGEN_OK: c_int = 0;
const COORDGEN_ERR_NULL: c_int = 1;
const COORDGEN_ERR_EMPTY: c_int = 2;
const COORDGEN_ERR_BAD_INDEX: c_int = 3;
const COORDGEN_ERR_EXCEPTION: c_int = 4;

#[repr(C)]
struct CoordgenInput {
    atom_count: c_int,
    atomic_numbers: *const c_int,
    formal_charges: *const c_int,
    bond_count: c_int,
    bond_from: *const c_int,
    bond_to: *const c_int,
    bond_orders: *const c_int,
    bond_stereos: *const c_int,
    bond_stereo_atom1: *const c_int,
    bond_stereo_atom2: *const c_int,
    has_template_match: *const c_int,
    is_atom_fixed: *const c_int,
    input_x: *const f32,
    input_y: *const f32,
    layout_flags: c_int,
}

#[repr(C)]
struct CoordgenOutput {
    out_x: *mut f32,
    out_y: *mut f32,
}

extern "C" {
    fn coordgen_generate_2d_ffi(input: *const CoordgenInput, output: *mut CoordgenOutput)
        -> c_int;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoordGenError {
    #[error("null pointer passed to CoordGen shim")]
    Null,
    #[error("empty molecule")]
    Empty,
    #[error("bond or atom index out of range / length mismatch")]
    BadIndex,
    #[error("C++ exception caught in CoordGen shim")]
    Exception,
    #[error("unknown CoordGen error code {0}")]
    Unknown(i32),
}

impl CoordGenError {
    fn from_code(code: c_int) -> Self {
        match code {
            COORDGEN_ERR_NULL => Self::Null,
            COORDGEN_ERR_EMPTY => Self::Empty,
            COORDGEN_ERR_BAD_INDEX => Self::BadIndex,
            COORDGEN_ERR_EXCEPTION => Self::Exception,
            other => Self::Unknown(other),
        }
    }
}

/// Flat molecule graph for 2D layout (chemistry-facing).
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutMolecule {
    pub atomic_numbers: Vec<i32>,
    pub formal_charges: Vec<i32>,
    pub bond_from: Vec<i32>,
    pub bond_to: Vec<i32>,
    pub bond_orders: Vec<i32>,
    /// [`BOND_STEREO_NONE`] / [`BOND_STEREO_CIS`] / [`BOND_STEREO_TRANS`]
    pub bond_stereos: Vec<i32>,
    /// Stereo substituent on begin side (optional; with [`Self::bond_stereo_atom2`]).
    pub bond_stereo_atom1: Option<Vec<i32>>,
    /// Stereo substituent on end side.
    pub bond_stereo_atom2: Option<Vec<i32>>,
    /// Soft template constraint (`atom->constrained`).
    pub has_template_match: Option<Vec<i32>>,
    /// Hard fix (`atom->fixed`).
    pub is_atom_fixed: Option<Vec<i32>>,
    pub input_x: Option<Vec<f32>>,
    pub input_y: Option<Vec<f32>>,
    pub layout_flags: i32,
}

impl Default for LayoutMolecule {
    fn default() -> Self {
        Self {
            atomic_numbers: Vec::new(),
            formal_charges: Vec::new(),
            bond_from: Vec::new(),
            bond_to: Vec::new(),
            bond_orders: Vec::new(),
            bond_stereos: Vec::new(),
            bond_stereo_atom1: None,
            bond_stereo_atom2: None,
            has_template_match: None,
            is_atom_fixed: None,
            input_x: None,
            input_y: None,
            layout_flags: 0,
        }
    }
}

impl LayoutMolecule {
    pub fn n_atoms(&self) -> usize {
        self.atomic_numbers.len()
    }

    pub fn n_bonds(&self) -> usize {
        self.bond_from.len()
    }

    pub fn ethanol() -> Self {
        Self {
            atomic_numbers: vec![6, 6, 8],
            formal_charges: vec![0, 0, 0],
            bond_from: vec![0, 1],
            bond_to: vec![1, 2],
            bond_orders: vec![1, 1],
            bond_stereos: vec![0, 0],
            ..Default::default()
        }
    }

    pub fn benzene() -> Self {
        Self {
            atomic_numbers: vec![6; 6],
            formal_charges: vec![0; 6],
            bond_from: vec![0, 1, 2, 3, 4, 5],
            bond_to: vec![1, 2, 3, 4, 5, 0],
            bond_orders: vec![1, 2, 1, 2, 1, 2],
            bond_stereos: vec![0; 6],
            ..Default::default()
        }
    }

    fn validate(&self) -> Result<(), CoordGenError> {
        let n = self.n_atoms();
        if n == 0 {
            return Err(CoordGenError::Empty);
        }
        if self.formal_charges.len() != n {
            return Err(CoordGenError::BadIndex);
        }
        let nb = self.n_bonds();
        if self.bond_to.len() != nb
            || self.bond_orders.len() != nb
            || self.bond_stereos.len() != nb
        {
            return Err(CoordGenError::BadIndex);
        }
        for opt in [
            self.has_template_match.as_ref(),
            self.is_atom_fixed.as_ref(),
        ] {
            if let Some(v) = opt {
                if v.len() != n {
                    return Err(CoordGenError::BadIndex);
                }
            }
        }
        for opt in [self.input_x.as_ref(), self.input_y.as_ref()] {
            if let Some(v) = opt {
                if v.len() != n {
                    return Err(CoordGenError::BadIndex);
                }
            }
        }
        match (&self.bond_stereo_atom1, &self.bond_stereo_atom2) {
            (None, None) => {}
            (Some(a), Some(b)) if a.len() == nb && b.len() == nb => {}
            _ => return Err(CoordGenError::BadIndex),
        }
        let needs_xy = self
            .has_template_match
            .as_ref()
            .is_some_and(|v| v.iter().any(|&x| x != 0))
            || self
                .is_atom_fixed
                .as_ref()
                .is_some_and(|v| v.iter().any(|&x| x != 0));
        if needs_xy && (self.input_x.is_none() || self.input_y.is_none()) {
            return Err(CoordGenError::BadIndex);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutResult {
    pub x: Vec<f32>,
    pub y: Vec<f32>,
}

impl LayoutResult {
    pub fn coords(&self) -> Vec<[f32; 2]> {
        self.x
            .iter()
            .zip(self.y.iter())
            .map(|(&x, &y)| [x, y])
            .collect()
    }
}

/// Generate 2D coordinates via statically linked CoordGen.
pub fn generate_2d_coordinates(mol: &LayoutMolecule) -> Result<LayoutResult, CoordGenError> {
    mol.validate()?;

    let n = mol.n_atoms();
    let nb = mol.n_bonds();
    let mut out_x = vec![0.0f32; n];
    let mut out_y = vec![0.0f32; n];

    let input = CoordgenInput {
        atom_count: n as c_int,
        atomic_numbers: mol.atomic_numbers.as_ptr(),
        formal_charges: mol.formal_charges.as_ptr(),
        bond_count: nb as c_int,
        bond_from: if nb == 0 {
            null()
        } else {
            mol.bond_from.as_ptr()
        },
        bond_to: if nb == 0 {
            null()
        } else {
            mol.bond_to.as_ptr()
        },
        bond_orders: if nb == 0 {
            null()
        } else {
            mol.bond_orders.as_ptr()
        },
        bond_stereos: if nb == 0 {
            null()
        } else {
            mol.bond_stereos.as_ptr()
        },
        bond_stereo_atom1: mol
            .bond_stereo_atom1
            .as_ref()
            .map_or(null(), |v| v.as_ptr()),
        bond_stereo_atom2: mol
            .bond_stereo_atom2
            .as_ref()
            .map_or(null(), |v| v.as_ptr()),
        has_template_match: mol
            .has_template_match
            .as_ref()
            .map_or(null(), |v| v.as_ptr()),
        is_atom_fixed: mol.is_atom_fixed.as_ref().map_or(null(), |v| v.as_ptr()),
        input_x: mol.input_x.as_ref().map_or(null(), |v| v.as_ptr()),
        input_y: mol.input_y.as_ref().map_or(null(), |v| v.as_ptr()),
        layout_flags: mol.layout_flags,
    };

    let mut output = CoordgenOutput {
        out_x: out_x.as_mut_ptr(),
        out_y: out_y.as_mut_ptr(),
    };

    let rc = unsafe { coordgen_generate_2d_ffi(&input, &mut output) };
    if rc != COORDGEN_OK {
        return Err(CoordGenError::from_code(rc));
    }

    Ok(LayoutResult {
        x: out_x,
        y: out_y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_layout(mol: &LayoutMolecule) {
        let res = generate_2d_coordinates(mol).expect("layout");
        assert_eq!(res.x.len(), mol.n_atoms());
        let any = res
            .x
            .iter()
            .zip(res.y.iter())
            .any(|(&x, &y)| x.abs() > 1e-3 || y.abs() > 1e-3);
        assert!(any, "expected non-trivial coords");
    }

    #[test]
    fn ethanol() {
        assert_layout(&LayoutMolecule::ethanol());
    }

    #[test]
    fn benzene() {
        assert_layout(&LayoutMolecule::benzene());
    }

    #[test]
    fn quick_precision_flag() {
        let mut mol = LayoutMolecule::benzene();
        mol.layout_flags = FLAG_QUICK_PRECISION;
        assert_layout(&mol);
    }

    #[test]
    fn even_angles_and_macrocycle_flags() {
        let mut mol = LayoutMolecule::benzene();
        mol.layout_flags = FLAG_EVEN_ANGLES | FLAG_FORCE_OPEN_MACROCYCLES;
        assert_layout(&mol);
    }

    #[test]
    fn fixed_atom_uses_input_coords() {
        let mut mol = LayoutMolecule::ethanol();
        mol.is_atom_fixed = Some(vec![1, 0, 0]);
        mol.input_x = Some(vec![10.0, 0.0, 0.0]);
        mol.input_y = Some(vec![20.0, 0.0, 0.0]);
        let res = generate_2d_coordinates(&mol).expect("layout");
        assert!((res.x[0] - 10.0).abs() < 5.0, "{:?}", res.x[0]);
        assert!((res.y[0] - 20.0).abs() < 5.0, "{:?}", res.y[0]);
    }

    #[test]
    fn constrained_ring_atoms_stay_near_seed() {
        // Hexagon seeds for first six carbons; seventh atom free (like toluene Me).
        let mut mol = LayoutMolecule {
            atomic_numbers: vec![6, 6, 6, 6, 6, 6, 6],
            formal_charges: vec![0; 7],
            bond_from: vec![0, 1, 2, 3, 4, 5, 0],
            bond_to: vec![1, 2, 3, 4, 5, 0, 6],
            bond_orders: vec![1, 2, 1, 2, 1, 2, 1],
            bond_stereos: vec![0; 7],
            ..Default::default()
        };
        let seeds_x = [0.0, 50.0, 75.0, 50.0, 0.0, -25.0, 0.0];
        let seeds_y = [0.0, 0.0, 43.0, 86.0, 86.0, 43.0, 0.0];
        mol.has_template_match = Some(vec![1, 1, 1, 1, 1, 1, 0]);
        mol.input_x = Some(seeds_x.to_vec());
        mol.input_y = Some(seeds_y.to_vec());
        let res = generate_2d_coordinates(&mol).expect("layout");
        for i in 0..6 {
            assert!(
                (res.x[i] - seeds_x[i]).abs() < 8.0,
                "atom {i}: got {} want {}",
                res.x[i],
                seeds_x[i]
            );
            assert!(
                (res.y[i] - seeds_y[i]).abs() < 8.0,
                "atom {i}: got {} want {}",
                res.y[i],
                seeds_y[i]
            );
        }
    }
}

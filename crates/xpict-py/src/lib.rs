//! PyO3 extension module `xpict._native`.
//!
//! Template: add `#[pyfunction]` wrappers here, keep logic in `xpict-core`.

#![forbid(unsafe_code)]

use pyo3::prelude::*;
use pyo3::types::PyModule;
use xpict_core::bonds;
use xpict_core::geom;
use xpict_core::metrics;
use xpict_core::plotdot::PlotDot;

/// Parallel spacing for double/triple strokes (RDKit `multipleBondOffset`).
#[pyfunction]
fn multi_bond_offset(length: f64) -> f64 {
    bonds::multi_bond_offset(length)
}

/// Signed offsets of a centered multiple bond along the left normal.
#[pyfunction]
fn centered_displacements(order: f64, off: f64) -> Vec<f64> {
    bonds::centered_displacements(order, off)
}

/// PlotDot rings for one normalized score `z`.
#[pyfunction]
#[pyo3(signature = (z, levels=4))]
fn plotdot_rings(z: f64, levels: usize) -> Vec<(f64, f64)> {
    PlotDot::new(levels).rings(z)
}

/// PlotDot disks: list of `(radius_frac, color_z, x, y)`.
#[pyfunction]
#[pyo3(signature = (zs, coords, levels=4))]
fn plotdot_disks(zs: Vec<f64>, coords: Vec<(f64, f64)>, levels: usize) -> Vec<(f64, f64, f64, f64)> {
    PlotDot::new(levels)
        .disks(&zs, &coords)
        .into_iter()
        .map(|d| (d.radius_frac, d.color_z, d.x, d.y))
        .collect()
}

/// SVG path `d` for a capsule ink halo (bond shaft knockout).
#[pyfunction]
#[pyo3(signature = (x1, y1, x2, y2, ink_radius, grow))]
fn capsule_halo_path_d(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    ink_radius: f64,
    grow: f64,
) -> Option<String> {
    geom::capsule_halo_path_d(x1, y1, x2, y2, ink_radius, grow)
}

/// SVG path `d` for a disk ink halo.
#[pyfunction]
#[pyo3(signature = (cx, cy, ink_radius, grow))]
fn disk_halo_path_d(cx: f64, cy: f64, ink_radius: f64, grow: f64) -> Option<String> {
    geom::disk_halo_path_d(cx, cy, ink_radius, grow)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(multi_bond_offset, m)?)?;
    m.add_function(wrap_pyfunction!(centered_displacements, m)?)?;
    m.add_function(wrap_pyfunction!(plotdot_rings, m)?)?;
    m.add_function(wrap_pyfunction!(plotdot_disks, m)?)?;
    m.add_function(wrap_pyfunction!(capsule_halo_path_d, m)?)?;
    m.add_function(wrap_pyfunction!(disk_halo_path_d, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("BOND_PX", metrics::BOND_PX)?;
    m.add("OFFSET_PX", metrics::OFFSET_PX)?;
    m.add("STROKE_PX", metrics::STROKE_PX)?;
    m.add("SHADE_FRAC", metrics::SHADE_FRAC)?;
    Ok(())
}

//! wasm-bindgen surface for **xpict** — same callable set as `xpict._native` (PyO3).
//!
//! Keep signatures parallel so JS and Python call the same `xpict-core` logic.
//! Vectors of pairs are flattened (`[r,c, r,c, …]`) for easy `Float64Array` use.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;
use xpict_core::align;
use xpict_core::bonds;
use xpict_core::depict;
use xpict_core::geom;
use xpict_core::metrics;
use xpict_core::plotdot::PlotDot;
use xpict_core::scene::MoleculeIn;

#[wasm_bindgen(js_name = multiBondOffset)]
pub fn multi_bond_offset(length: f64) -> f64 {
    bonds::multi_bond_offset(length)
}

#[wasm_bindgen(js_name = centeredDisplacements)]
pub fn centered_displacements(order: f64, off: f64) -> Vec<f64> {
    bonds::centered_displacements(order, off)
}

/// `MoleculeIn` JSON → `Scene` JSON (MVP paint ABI).
#[wasm_bindgen(js_name = depictMolecule)]
pub fn depict_molecule(molecule_json: &str) -> Result<String, JsValue> {
    let mol: MoleculeIn = serde_json::from_str(molecule_json)
        .map_err(|e| JsValue::from_str(&format!("MoleculeIn JSON: {e}")))?;
    let scene = depict::depict_molecule(&mol);
    serde_json::to_string(&scene).map_err(|e| JsValue::from_str(&format!("Scene JSON: {e}")))
}

/// Kabsch 2D. Flat `[x,y,…]` pairs for src and dst.
/// Returns `[cos, sin, tx, ty, det]`.
#[wasm_bindgen(js_name = kabsch2d)]
pub fn kabsch_2d(src: Vec<f64>, dst: Vec<f64>, allow_reflect: bool) -> Result<Vec<f64>, JsValue> {
    if src.len() != dst.len() || src.len() % 2 != 0 {
        return Err(JsValue::from_str("src/dst must be equal-length flat [x,y,…]"));
    }
    let n = src.len() / 2;
    let mut s = Vec::with_capacity(n);
    let mut d = Vec::with_capacity(n);
    for i in 0..n {
        s.push((src[2 * i], src[2 * i + 1]));
        d.push((dst[2 * i], dst[2 * i + 1]));
    }
    let xf = align::kabsch_2d(&s, &d, allow_reflect);
    Ok(vec![xf.cos, xf.sin, xf.tx, xf.ty, xf.det])
}

/// Rigid-align other coords onto a template.
///
/// `template` / `other`: flat `[index, x, y, …]`.
/// `mapping`: flat `[otherIndex, templateIndex, …]`.
/// Returns flat `[index, x, y, …]` for the transformed other atoms.
#[wasm_bindgen(js_name = rigidAlignCoords)]
pub fn rigid_align_coords(
    template: Vec<f64>,
    other: Vec<f64>,
    mapping: Vec<i32>,
) -> Result<Vec<f64>, JsValue> {
    let parse_atoms = |flat: &[f64], label: &str| -> Result<Vec<(i32, f64, f64)>, JsValue> {
        if flat.len() % 3 != 0 {
            return Err(JsValue::from_str(&format!(
                "{label} must be flat [index, x, y, …]"
            )));
        }
        Ok((0..flat.len() / 3)
            .map(|i| (flat[3 * i] as i32, flat[3 * i + 1], flat[3 * i + 2]))
            .collect())
    };
    if mapping.len() % 2 != 0 {
        return Err(JsValue::from_str(
            "mapping must be flat [otherIndex, templateIndex, …]",
        ));
    }
    let tmpl = parse_atoms(&template, "template")?;
    let oth = parse_atoms(&other, "other")?;
    let map: Vec<(i32, i32)> = (0..mapping.len() / 2)
        .map(|i| (mapping[2 * i], mapping[2 * i + 1]))
        .collect();
    let (aligned, _) = align::rigid_align_coords(&tmpl, &oth, &map);
    let mut out = Vec::with_capacity(aligned.len() * 3);
    for (i, x, y) in aligned {
        out.push(i as f64);
        out.push(x);
        out.push(y);
    }
    Ok(out)
}

/// Flat `[radius, color, …]` for one score (empty when near zero).
#[wasm_bindgen(js_name = plotdotRings)]
pub fn plotdot_rings(z: f64, levels: usize) -> Vec<f64> {
    let mut out = Vec::new();
    for (r, c) in PlotDot::new(levels).rings(z) {
        out.push(r);
        out.push(c);
    }
    out
}

/// Flat `[radius, color, x, y, …]` sorted weak→strong.
#[wasm_bindgen(js_name = plotdotDisks)]
pub fn plotdot_disks(zs: Vec<f64>, coords: Vec<f64>, levels: usize) -> Vec<f64> {
    let n = zs.len().min(coords.len() / 2);
    let pairs: Vec<(f64, f64)> = (0..n)
        .map(|i| (coords[2 * i], coords[2 * i + 1]))
        .collect();
    let mut out = Vec::new();
    for d in PlotDot::new(levels).disks(&zs[..n], &pairs) {
        out.push(d.radius_frac);
        out.push(d.color_z);
        out.push(d.x);
        out.push(d.y);
    }
    out
}

#[wasm_bindgen(js_name = capsuleHaloPathD)]
pub fn capsule_halo_path_d(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    ink_radius: f64,
    grow: f64,
) -> Option<String> {
    geom::capsule_halo_path_d(x1, y1, x2, y2, ink_radius, grow)
}

#[wasm_bindgen(js_name = diskHaloPathD)]
pub fn disk_halo_path_d(cx: f64, cy: f64, ink_radius: f64, grow: f64) -> Option<String> {
    geom::disk_halo_path_d(cx, cy, ink_radius, grow)
}

#[wasm_bindgen(js_name = bondPx)]
pub fn bond_px() -> f64 {
    metrics::BOND_PX
}

#[wasm_bindgen(js_name = offsetPx)]
pub fn offset_px() -> f64 {
    metrics::OFFSET_PX
}

#[wasm_bindgen(js_name = strokePx)]
pub fn stroke_px() -> f64 {
    metrics::STROKE_PX
}

#[wasm_bindgen(js_name = shadeFrac)]
pub fn shade_frac() -> f64 {
    metrics::SHADE_FRAC
}

#[wasm_bindgen(js_name = elementSymbol)]
pub fn element_symbol(z: u32) -> String {
    xpict_core::element_symbol(z).to_string()
}

#[wasm_bindgen(js_name = atomicNumber)]
pub fn atomic_number(symbol: &str) -> Option<u32> {
    xpict_core::atomic_number(symbol)
}

#[wasm_bindgen(js_name = coreVersion)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

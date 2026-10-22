//! wasm-bindgen surface for **xpict** — one [`render`] entry for every spec.
//!
//! CoordGen layout, mol-spec paint (chem + annotations + caption), and
//! scheme compose all live in core. Hosts pass DepictSpec JSON.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;
use xpict_core::scene::MoleculeIn;
use xpict_core::DepictSpec;

/// `MoleculeIn` JSON → `Scene` JSON (chem paint only; prefer [`render`]).
#[wasm_bindgen(js_name = depictMolecule)]
pub fn depict_molecule(molecule_json: &str) -> Result<String, JsValue> {
    let mol: MoleculeIn = serde_json::from_str(molecule_json)
        .map_err(|e| JsValue::from_str(&format!("MoleculeIn JSON: {e}")))?;
    let scene = xpict_core::depict_molecule(&mol);
    serde_json::to_string(&scene).map_err(|e| JsValue::from_str(&format!("Scene JSON: {e}")))
}

/// DepictSpec JSON → `Rendered[]` JSON.
#[wasm_bindgen(js_name = render)]
pub fn render(spec_json: &str) -> Result<String, JsValue> {
    let spec: DepictSpec = serde_json::from_str(spec_json)
        .map_err(|e| JsValue::from_str(&format!("DepictSpec JSON: {e}")))?;
    let painted = xpict_core::doc::render(&spec).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&painted).map_err(|e| JsValue::from_str(&format!("Rendered JSON: {e}")))
}

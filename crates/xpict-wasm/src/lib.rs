//! wasm-bindgen surface for **xpict** — MVP paint only.
//!
//! JS public API is ``xpict.mol`` / ``render`` / ``toSvg``; this module only
//! exposes ``depictMolecule`` for that path. Broader helpers stay in
//! ``xpict-core`` / PyO3.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;
use xpict_core::depict;
use xpict_core::scene::MoleculeIn;

/// `MoleculeIn` JSON → `Scene` JSON (MVP paint ABI).
#[wasm_bindgen(js_name = depictMolecule)]
pub fn depict_molecule(molecule_json: &str) -> Result<String, JsValue> {
    let mol: MoleculeIn = serde_json::from_str(molecule_json)
        .map_err(|e| JsValue::from_str(&format!("MoleculeIn JSON: {e}")))?;
    let scene = depict::depict_molecule(&mol);
    serde_json::to_string(&scene).map_err(|e| JsValue::from_str(&format!("Scene JSON: {e}")))
}

//! wasm-bindgen surface for **xpict** — paint + document two-pass.
//!
//! Document path: ``planEdge`` → host ``processEdgePlan`` → ``renderDoc``.
//! CX / star / shade chrome lives in core ``render_doc`` — not client code.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;
use xpict_core::scene::MoleculeIn;

/// `MoleculeIn` JSON → `Scene` JSON (MVP paint ABI).
#[wasm_bindgen(js_name = depictMolecule)]
pub fn depict_molecule(molecule_json: &str) -> Result<String, JsValue> {
    let mol: MoleculeIn = serde_json::from_str(molecule_json)
        .map_err(|e| JsValue::from_str(&format!("MoleculeIn JSON: {e}")))?;
    let scene = xpict_core::depict_molecule(&mol);
    serde_json::to_string(&scene).map_err(|e| JsValue::from_str(&format!("Scene JSON: {e}")))
}

/// Pass 1: `DepictSpec` JSON → `EdgePlan` JSON (or ``null`` when empty).
#[wasm_bindgen(js_name = planEdge)]
pub fn plan_edge(spec_json: &str) -> Result<String, JsValue> {
    let spec: xpict_core::DepictSpec = serde_json::from_str(spec_json)
        .map_err(|e| JsValue::from_str(&format!("DepictSpec JSON: {e}")))?;
    let plan = xpict_core::plan_edge(&spec).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&plan).map_err(|e| JsValue::from_str(&format!("EdgePlan JSON: {e}")))
}

/// Pass 2: `DepictSpec` + `EdgeResult` JSON → DocPaint list JSON.
#[wasm_bindgen(js_name = renderDoc)]
pub fn render_doc(spec_json: &str, edge_json: &str) -> Result<String, JsValue> {
    let spec: xpict_core::DepictSpec = serde_json::from_str(spec_json)
        .map_err(|e| JsValue::from_str(&format!("DepictSpec JSON: {e}")))?;
    let edge: xpict_core::EdgeResult = serde_json::from_str(edge_json)
        .map_err(|e| JsValue::from_str(&format!("EdgeResult JSON: {e}")))?;
    let painted =
        xpict_core::render_doc(&spec, &edge).map_err(|e| JsValue::from_str(&e))?;
    let rows: Vec<serde_json::Value> = painted
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "molecule": p.molecule,
                "scene": p.scene,
            })
        })
        .collect();
    serde_json::to_string(&rows).map_err(|e| JsValue::from_str(&format!("DocPaint JSON: {e}")))
}

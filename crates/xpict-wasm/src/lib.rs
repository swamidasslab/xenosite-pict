//! wasm-bindgen surface for **xpict** — one [`render`] entry for every spec.
//!
//! CoordGen layout, mol-spec paint (chem + annotations + caption), and
//! scheme compose all live in core. Hosts pass DepictSpec JSON.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;
use xpict_core::{DepictSpec, MolNode};

fn mol_node_from_source(source: &str) -> MolNode {
    let text = source.trim();
    let mut node = MolNode::default();
    if text.contains("V2000") || text.contains("V3000") || text.lines().count() >= 4 {
        node.molfile = Some(text.to_string());
    } else if text.contains('|') {
        node.cxsmiles = Some(text.to_string());
    } else {
        node.smiles = Some(text.to_string());
    }
    node
}

fn spec_from_json(json: &str) -> Result<DepictSpec, JsValue> {
    if let Ok(spec) = serde_json::from_str::<DepictSpec>(json) {
        return Ok(spec);
    }
    if let Ok(node) = serde_json::from_str::<MolNode>(json) {
        return Ok(DepictSpec::Mol(node));
    }
    if let Ok(source) = serde_json::from_str::<String>(json) {
        return Ok(DepictSpec::Mol(mol_node_from_source(&source)));
    }
    Err(JsValue::from_str(
        "DepictSpec JSON: expected a mol/group/scheme/grid, a MolNode, or a source string",
    ))
}

fn render_spec(spec: &DepictSpec) -> Result<String, JsValue> {
    let painted = xpict_core::doc::render(spec).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&painted).map_err(|e| JsValue::from_str(&format!("Rendered JSON: {e}")))
}

/// Single mol (source string, `MolNode`, or `{smiles}`) → `Rendered[]` JSON.
///
/// Translates to a DepictSpec and calls [`render`]. Not a parallel paint path.
#[wasm_bindgen(js_name = depictMolecule)]
pub fn depict_molecule(mol_json: &str) -> Result<String, JsValue> {
    render_spec(&spec_from_json(mol_json)?)
}

/// DepictSpec JSON → `Rendered[]` JSON.
#[wasm_bindgen(js_name = render)]
pub fn render(spec_json: &str) -> Result<String, JsValue> {
    render_spec(&spec_from_json(spec_json)?)
}

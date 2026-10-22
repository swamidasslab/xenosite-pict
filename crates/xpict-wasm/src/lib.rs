//! Emscripten C ABI for **xpict** — one [`xpict_render`] entry for every spec.
//!
//! CoordGen layout, mol-spec paint (chem + annotations + caption), and
//! scheme compose all live in core. Hosts pass DepictSpec JSON.
//!
//! Built as `wasm32-unknown-emscripten` so CoordGen's C++ (`em++`) links
//! libc++. `wasm32-unknown-unknown` cannot compile `<map>`.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

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

fn spec_from_json(json: &str) -> Result<DepictSpec, String> {
    if let Ok(spec) = serde_json::from_str::<DepictSpec>(json) {
        return Ok(spec);
    }
    if let Ok(node) = serde_json::from_str::<MolNode>(json) {
        return Ok(DepictSpec::Mol(node));
    }
    if let Ok(source) = serde_json::from_str::<String>(json) {
        return Ok(DepictSpec::Mol(mol_node_from_source(&source)));
    }
    Err("DepictSpec JSON: expected a mol/group/scheme/grid, a MolNode, or a source string".into())
}

fn render_spec(spec: &DepictSpec) -> Result<String, String> {
    let painted = xpict_core::doc::render(spec)?;
    serde_json::to_string(&painted).map_err(|e| format!("Rendered JSON: {e}"))
}

fn cstring_ptr(s: String) -> *mut c_char {
    CString::new(s.replace('\0', ""))
        .unwrap_or_else(|_| CString::new("xpict: interior NUL").expect("static"))
        .into_raw()
}

fn read_cstr<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err("null pointer".into());
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| "argument is not utf-8".into())
}

/// DepictSpec JSON → `Rendered[]` JSON. Caller frees with [`xpict_string_free`].
#[no_mangle]
pub extern "C" fn xpict_render(spec_json: *const c_char) -> *mut c_char {
    let out = (|| {
        let json = read_cstr(spec_json)?;
        render_spec(&spec_from_json(json)?)
    })();
    match out {
        Ok(s) => cstring_ptr(s),
        Err(e) => cstring_ptr(e),
    }
}

/// Single mol JSON → spec → [`xpict_render`]. Caller frees with [`xpict_string_free`].
#[no_mangle]
pub extern "C" fn xpict_depict_molecule(mol_json: *const c_char) -> *mut c_char {
    xpict_render(mol_json)
}

/// Free a string returned by [`xpict_render`].
#[no_mangle]
pub extern "C" fn xpict_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

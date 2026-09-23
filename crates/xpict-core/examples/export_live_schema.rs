//! Export live JSON Schema from Rust (schemars): EdgePlan / EdgeResult / Scene / DepictSpec.
//!
//! ```text
//! cargo run -p xpict-core --example export_live_schema --features codegen
//! ```

use std::fs;
use std::path::PathBuf;

use schemars::schema_for;
use xpict_core::{DepictSpec, EdgePlan, EdgeResult, Scene};

fn write(root: &std::path::Path, name: &str, schema: schemars::schema::RootSchema) {
    let path = root.join(name);
    fs::write(
        &path,
        serde_json::to_string_pretty(&schema).expect("serialize schema") + "\n",
    )
    .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!("wrote {}", path.display());
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("schema");
    fs::create_dir_all(&root).expect("schema dir");

    write(&root, "edge-plan.schema.json", schema_for!(EdgePlan));
    write(&root, "edge-result.schema.json", schema_for!(EdgeResult));
    write(&root, "scene.schema.json", schema_for!(Scene));
    write(&root, "xpict.schema.json", schema_for!(DepictSpec));
}

//! Export live EdgePlan / EdgeResult JSON Schema from Rust (schemars).
//!
//! ```text
//! cargo run -p xpict-core --example export_live_schema --features codegen
//! ```

use std::fs;
use std::path::PathBuf;

use schemars::schema_for;
use xpict_core::{EdgePlan, EdgeResult};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("schema");
    fs::create_dir_all(&root).expect("schema dir");

    let plan = schema_for!(EdgePlan);
    let result = schema_for!(EdgeResult);

    let plan_path = root.join("edge-plan.schema.json");
    let result_path = root.join("edge-result.schema.json");
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize plan schema") + "\n",
    )
    .expect("write edge-plan.schema.json");
    fs::write(
        &result_path,
        serde_json::to_string_pretty(&result).expect("serialize result schema") + "\n",
    )
    .expect("write edge-result.schema.json");

    println!("wrote {}", plan_path.display());
    println!("wrote {}", result_path.display());
}

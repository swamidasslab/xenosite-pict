# xpict

Public Rust API for molecule depiction — mirrors the JS (`xpict.mol` /
`render` / `toSvg`) and Python (`Mol` / `Rendered`) clients.

**Layout** uses the crates.io [`rdkit`](https://crates.io/crates/rdkit) package
(SMILES / molblock) plus a small local C++ bridge for
`RDDepict::compute2DCoords` / `generateDepictionMatching2DStructure` and bond
iteration (not yet exposed by high-level `rdkit`). **Paint** is
[`xpict-core`](../xpict-core) (`depict_molecule` → scene → SVG).

This crate is a **separate workspace member**. It is **not** a dependency of
`xpict-py` or `xpict-wasm`, so RDKit stays out of the Python extension and the
browser WASM blob.

## Usage

```rust
use xpict::{mol, MolRenderOptions};

fn main() -> Result<(), xpict::Error> {
    let mut m = mol("CCO")?;
    let rendered = m.render(MolRenderOptions::default())?;
    println!("{}", rendered.to_svg());
    Ok(())
}
```

Align onto a prior pose:

```rust
use xpict::{mol, MolRenderOptions};

let mut ethanol = mol("CCO")?;
let home = ethanol.render(MolRenderOptions::default())?;
let mut propanol = mol("CCCO")?;
let aligned = propanol.render(MolRenderOptions {
    align_to: Some(home.frame().to_string()),
    ..Default::default()
})?;
```

## System requirements

- RDKit C++ libraries + headers (e.g. Ubuntu `librdkit-dev`, or the prebuilt
  archives documented by [`rdkit`](https://crates.io/crates/rdkit))
- Boost (serialization) headers/libs
- A C++17 compiler (`g++`)

Ubuntu 24.04 `librdkit-dev` **202309** is missing `GraphMol/FileParsers/FileWriters.h`
(expected by `rdkit-sys` 0.4.x). This crate ships a compatibility shim under
`compat/rdkit/`; point `CPLUS_INCLUDE_PATH` at it before building (or use
`.cargo/config.toml` in this repo).

```bash
export CPLUS_INCLUDE_PATH="$(pwd)/crates/xpict/compat/rdkit${CPLUS_INCLUDE_PATH:+:$CPLUS_INCLUDE_PATH}"
export CXX=g++
cargo test -p xpict
```

## Workspace note

Root `Cargo.toml` lists this package as a member but keeps
`default-members = ["crates/xpict-core"]` so plain `cargo test` / CI coverage
does not require system RDKit.

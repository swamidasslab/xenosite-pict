# xpict (native Rust)

Public Rust API mirroring JS/Python: **`mol` / `render` / `to_svg`**, plus a
batch stub **`depict({ molecules }) → Vec<Rendered>`**.

**Layout** uses crates.io [`rdkit`](https://crates.io/crates/rdkit) plus a
local Depictor FFI. **Paint** is [`xpict-core`](../xpict-core). This crate is
**not** a dependency of `xpict-py` / `xpict-wasm`.

## Usage

```rust
use xpict::{depict, mol, DepictSpec, MolRenderOptions, MolSpec};

fn main() -> Result<(), xpict::Error> {
    let mut m = mol("CCO")?;
    let rendered = m.render(MolRenderOptions {
        mark_atoms: Some(vec![2]),
        ..Default::default()
    })?;
    println!("{}", rendered.to_svg());

    let batch = depict(&DepictSpec {
        molecules: vec![
            MolSpec {
                smiles: Some("CCO".into()),
                mark_atoms: Some(vec![2]),
                ..Default::default()
            },
            MolSpec {
                smiles: Some("CCCO".into()),
                align_to: Some(0),
                ..Default::default()
            },
        ],
    })?;
    assert_eq!(batch.len(), 2);
    Ok(())
}
```

## System requirements

- RDKit C++ + Boost (e.g. Ubuntu `librdkit-dev`, `libboost-dev`)
- C++17 (`g++`)
- Ubuntu 202309: use the shipped `compat/rdkit` `FileWriters.h` shim
  (`CPLUS_INCLUDE_PATH` — see repo `.cargo/config.toml`)

```bash
cargo test -p xpict
```

## Publish

See [`docs/publish.md`](../../docs/publish.md). Publish **`xpict-core` first**,
then this crate. Not part of default workspace members / CI coverage (no RDKit
on that job).

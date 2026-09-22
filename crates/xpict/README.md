# xpict (native Rust)

**Declarative molecule depiction for publication-quality vector graphics** —
Rust API (`mol` / `render` / `to_svg`, plus batch `depict`), matching Python and
JavaScript.

**Layout:** crates.io [`rdkit`](https://crates.io/crates/rdkit) + Depictor FFI.  
**Paint:** [`xpict-core`](https://crates.io/crates/xpict-core) (no RDKit).  
**Docs:** [GitHub Pages](https://swamidasslab.github.io/xenosite-pict/) ·
[publish guide](https://github.com/swamidasslab/xenosite-pict/blob/main/docs/publish.md)

This crate is **not** linked into the Python or WASM packages (those use
`xpict-core` only).

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

- RDKit C++ libraries + headers, Boost, C++17 (`g++`)
- On some Ubuntu images, set `CPLUS_INCLUDE_PATH` to the repo
  `crates/xpict/compat/rdkit` shim (see `.cargo/config.toml` in the monorepo)

```bash
cargo test -p xpict
```

Publish **`xpict-core` first**, then this crate (`rust-core/v*` then `rust/v*`,
or `release/v*` for an all-language cut).

## Contributing

Issues and PRs welcome:
https://github.com/swamidasslab/xenosite-pict  

Future nested `PictSpec` design (comments welcome):
https://github.com/swamidasslab/xenosite-pict/tree/main/python/xpict/future  

## License

MIT

# xpict (native Rust)

**Declarative molecule depiction** — Rust API matching Python and JavaScript.

**Preferred:** nested [`DepictSpec`](https://docs.rs/xpict) (`mol` / `group`).  
**Simple:** `mol` / `render` / `to_svg`.

**Layout:** crates.io [`rdkit`](https://crates.io/crates/rdkit) + Depictor FFI.  
**Paint:** [`xpict-core`](https://crates.io/crates/xpict-core).  
**Docs:** [GitHub Pages](https://swamidasslab.github.io/xenosite-pict/) ·
[label markup](https://github.com/swamidasslab/xenosite-pict/blob/main/docs/label-markup.md)

```rust
use xpict::{depict, mol, DepictSpec, MolNode, MolRenderOptions};

fn main() -> Result<(), xpict::Error> {
    let batch = depict(&DepictSpec::Group {
        id: None,
        children: vec![MolNode {
            smiles: Some("CCO".into()),
            ..Default::default()
        }],
    })?;

    let mut m = mol("CCO")?;
    let rendered = m.render(MolRenderOptions {
        atom_shade: Some(vec![0.0, 0.2, 0.9]),
        ..Default::default()
    })?;
    println!("{}", rendered.to_svg());
    let _ = batch;
    Ok(())
}
```

```bash
cargo test -p xpict
```

## License

MIT

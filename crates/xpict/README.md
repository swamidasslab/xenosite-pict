# xpict (native Rust)

**Molecule depiction** — Rust API matching Python and JavaScript.

**Single molecule:** `mol` / `render` / `to_svg`.  
**Declarative document:** nested [`DepictSpec`](https://docs.rs/xpict) (`mol` /
`group` / `reaction_scheme` / `grid`).

**Layout + paint:** [`xpict-core`](https://crates.io/crates/xpict-core)
(Chematic + CoordGen). No system RDKit.  
**Docs:** [GitHub Pages](https://swamidasslab.github.io/xenosite-pict/) ·
[label markup](https://github.com/swamidasslab/xenosite-pict/blob/main/docs/label-markup.md)

```rust
use xpict::{depict, mol, DepictSpec, MolNode, MolRenderOptions, Node};

fn main() -> Result<(), xpict::Error> {
    let batch = depict(&DepictSpec::Group {
        id: None,
        align: false,
        children: vec![Node::Mol(MolNode {
            smiles: Some("CCO".into()),
            ..Default::default()
        })],
        opts: None,
        color: None,
        scale: None,
        aromatic: None,
        debug_box: None,
        debug_align: None,
        pad: None,
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

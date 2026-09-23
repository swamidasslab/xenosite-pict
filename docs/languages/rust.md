# Rust

| Crate | Role |
| --- | --- |
| [`xpict`](https://crates.io/crates/xpict) | Public API + RDKit layout (needs system RDKit + Boost) |
| [`xpict-core`](https://crates.io/crates/xpict-core) | Shared paint only (no RDKit) — used by WASM / PyO3 |

```toml
xpict = "0.1"
# or paint-only:
xpict-core = "0.1"
```

## Single molecule

```rust
use xpict::{mol, MolRenderOptions};

let mut home = mol("c1ccccc1")?;
let rendered = home.render(MolRenderOptions {
    color: Some("#0b6e4f".into()),
    atom_shade: Some(vec![0.0, 0.0, 0.2, 0.0, 0.0, 0.9]),
    star_labels: Some(vec![Some("$R_1$".into())]), // when source has *
    ..Default::default()
})?;
let svg = rendered.to_svg();
```

<div class="example-out" markdown>

<figure markdown="span">
![Benzene colored](../assets/examples/benzene_color.svg)
<figcaption>`color`</figcaption>
</figure>

<figure markdown="span">
![Star R₁](../assets/examples/star_r1.svg)
<figcaption>`star_labels`</figcaption>
</figure>

</div>

## Declarative document

```rust
use xpict::{depict, DepictSpec, MolNode};

let out = depict(&DepictSpec::Group {
    id: None,
    children: vec![
        MolNode {
            smiles: Some("CCO".into()),
            color: Some("#0b6e4f".into()),
            ..Default::default()
        },
        MolNode {
            cxsmiles: Some("*c1ccccc1Cl |$R_{1};;;;;$|".into()),
            ..Default::default()
        },
    ],
})?;
let svg = out[0].to_svg();
```

<div class="example-out" markdown>

<figure markdown="span">
![Ethanol with shading](../assets/examples/ethanol_shade.svg)
<figcaption>Ethanol</figcaption>
</figure>

<figure markdown="span">
![Markush](../assets/examples/markush.svg)
<figcaption>CX on a mol node</figcaption>
</figure>

</div>

- [rustdoc](../api/rust.md)
- [Label markup](../label-markup.md)
- [Install](../install.md)

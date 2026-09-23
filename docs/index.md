# xpict

**Declarative molecule depiction** for publication-quality vector graphics —
one Rust paint core (`xpict-core`), with RDKit layout at each language edge
(Python, JavaScript/WASM, native Rust).

[Browse the gallery](gallery.md) · [Install](install.md) · [API](api/overview.md)

## Preferred: declarative document

=== "JavaScript"

    ```js
    import { xpict } from "@xenosite/xpict";

    const [r] = await xpict.depict({
      type: "mol",
      smiles: "c1ccccc1O",
      shade: { atoms: [0, 0, 0, 0, 0, 0, 0.9], vmin: 0, vmax: 1 },
    });
    const svg = xpict.toSvg(r.scene);
    ```

=== "Python"

    ```python
    from xpict import render

    svg = render({
        "type": "mol",
        "smiles": "c1ccccc1O",
        "shade": {"atoms": [0, 0, 0, 0, 0, 0, 0.9], "vmin": 0, "vmax": 1},
    })
    ```

=== "Rust"

    ```rust
    use xpict::{depict, DepictSpec};

    let out = depict(&DepictSpec::Mol {
        smiles: Some("c1ccccc1O".into()),
        cxsmiles: None,
        molfile: None,
        id: None,
        color: None,
        shade: None,
        rgroups: None,
    })?;
    let svg = out[0].to_svg();
    ```

[Label markup](label-markup.md) · [API overview](api/overview.md) (simple client too).

## Live examples

<div class="gallery-grid" markdown="0">
  <figure>
    <img src="assets/examples/aspirin.svg" alt="Aspirin depiction" />
    <figcaption>Aspirin</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/caffeine.svg" alt="Caffeine depiction" />
    <figcaption>Caffeine</figcaption>
  </figure>
  <figure>
    <img src="assets/examples/markush.svg" alt="Markush R1 on chlorobenzene" />
    <figcaption>Markush ($R_1$)</figcaption>
  </figure>
</div>

<a class="md-button md-button--primary" href="gallery.md">More depictions</a>
<a class="md-button" href="js/demo/">JS align demo</a>

## Packages

| Language | Package | Autodoc |
| --- | --- | --- |
| Python | [`xpict`](https://pypi.org/project/xpict/) | [API reference](api/python.md) |
| JavaScript | [`@xenosite/xpict`](https://www.npmjs.com/package/@xenosite/xpict) | [TypeDoc](api/javascript.md) |
| Rust | [`xpict`](https://crates.io/crates/xpict) / [`xpict-core`](https://crates.io/crates/xpict-core) | [rustdoc](api/rust.md) |

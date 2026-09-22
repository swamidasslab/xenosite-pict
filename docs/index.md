# xpict

**Declarative molecule depiction** for publication-quality vector graphics —
one Rust paint core (`xpict-core`), with RDKit layout at each language edge
(Python, JavaScript/WASM, native Rust).

[Browse the gallery](gallery.md) · [Install](install.md) · [API](api/overview.md)

## Same three calls everywhere

=== "JavaScript"

    ```js
    import { xpict } from "@xenosite/xpict";

    const mol = xpict.mol("c1ccccc1O");
    const rendered = await xpict.render(mol, { mark_atoms: [6] });
    const svg = xpict.toSvg(rendered.scene);
    ```

=== "Python"

    ```python
    from xpict import render

    svg = render({"molecules": [{"smiles": "c1ccccc1O"}]})
    ```

=== "Rust"

    ```rust
    use xpict::mol;

    let svg = mol("c1ccccc1O")?.render()?.to_svg();
    ```

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

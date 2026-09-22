# API overview

The **shipped** cross-language surface is:

`mol` → `render(opts?)` → `Rendered` → `toSvg(scene)`

plus a batch stub `depict({ molecules })` → list of rendered molecules.

Nested diagrams / ELK / full `PictSpec` live under `xpict.future` and are
**not** the publish surface yet.

| Language | Autodoc |
| --- | --- |
| [Python](python.md) | mkdocstrings from package docstrings |
| [JavaScript](javascript.md) | TypeDoc from `js/src` |
| [Rust](rust.md) | rustdoc for `xpict-core` (and `xpict` when built) |

Quick examples:

=== "JavaScript"

    ```js
    import { xpict } from "@xenosite/xpict";
    const svg = xpict.toSvg(
      (await xpict.render(xpict.mol("CCO"))).scene
    );
    ```

=== "Python"

    ```python
    from xpict import render
    svg = render({"molecules": [{"smiles": "CCO"}]})
    ```

=== "Rust"

    ```rust
    // crates.io `xpict` (RDKit-backed)
    let svg = xpict::mol("CCO")?.render()?.to_svg();
    ```

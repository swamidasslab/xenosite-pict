# API overview

Two layers (same paint):

1. **Preferred — declarative document**  
   `depict({ molecules })` / `DepictSpec` (→ growing `PictSpec`).  
   Uses `mol` / `render` internally. This is the surface that keeps gaining
   functionality.

2. **Simple — single molecule**  
   `mol` → `render(opts?)` → `Rendered` → `toSvg(scene)`  
   For callers that only need one depiction. `align_to` here is a **Mol /
   Rendered** (or Rust pose molblock) — not a list index.

Nested diagrams / ELK / full `PictSpec` live under `xpict.future` until they
graduate into live contracts.

| Language | Autodoc |
| --- | --- |
| [Python](python.md) | mkdocstrings from package docstrings |
| [JavaScript](javascript.md) | TypeDoc from `js/src` |
| [Rust](rust.md) | rustdoc for `xpict-core` (and `xpict` when built) |

Quick examples:

=== "JavaScript"

    ```js
    import { xpict } from "@xenosite/xpict";

    // Preferred: declarative document
    const [r] = await xpict.depict({
      molecules: [{ smiles: "CCO", mark_atoms: [2] }],
    });
    const svg = xpict.toSvg(r.scene);

    // Simple: single mol
    // const svg = xpict.toSvg((await xpict.render(xpict.mol("CCO"))).scene);
    ```

=== "Python"

    ```python
    # Preferred: declarative document (Pict / DepictSpec-shaped)
    from xpict import render
    svg = render({"molecules": [{"smiles": "CCO", "mark_atoms": [2]}]})
    ```

=== "Rust"

    ```rust
    // Preferred: declarative document
    use xpict::{depict, DepictSpec, MolSpec};
    let out = depict(&DepictSpec {
        molecules: vec![MolSpec {
            smiles: Some("CCO".into()),
            ..Default::default()
        }],
    })?;
    let svg = out[0].to_svg();

    // Simple: mol.render().to_svg()
    ```

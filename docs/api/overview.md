# API overview

Two layers (same paint):

1. **Preferred — declarative document**  
   Nested subset of future `PictSpec`: `type: "mol"` or
   `type: "group"` + `children`.  
   Uses the simple client internally. Grows as features graduate from
   `xpict.future`.

2. **Simple — single molecule**  
   `mol` → `render(opts?)` → `Rendered` → `toSvg(scene)`  
   Imperative options (`color`, `atom_shade`, `star_labels`, `align_to`, …).

There is **no** top-level `molecules` list. Chem scripts:
[Label markup](../label-markup.md) (CX limits vs JSON `rgroups` /
`star_labels`).

| Language | Autodoc |
| --- | --- |
| [Python](python.md) | mkdocstrings from package docstrings |
| [JavaScript](javascript.md) | TypeDoc from `js/src` |
| [Rust](rust.md) | rustdoc for `xpict-core` (and `xpict` when built) |

### Preferred document

=== "JavaScript"

    ```js
    import { xpict } from "@xenosite/xpict";
    const [r] = await xpict.depict({
      type: "mol",
      smiles: "CCO",
      shade: { atoms: [0, 0.2, 0.9], vmin: 0, vmax: 1 },
    });
    const svg = xpict.toSvg(r.scene);
    ```

=== "Python"

    ```python
    from xpict import render
    svg = render({
        "type": "mol",
        "smiles": "CCO",
        "shade": {"atoms": [0.0, 0.2, 0.9], "vmin": 0.0, "vmax": 1.0},
    })
    ```

=== "Rust"

    ```rust
    use xpict::{depict, DepictSpec};
    let out = depict(&DepictSpec::Mol {
        smiles: Some("CCO".into()),
        ..Default::default()  // use struct update via helper in real code
    })?;
    let svg = out[0].to_svg();
    ```

### Simple client

=== "JavaScript"

    ```js
    const home = xpict.mol("c1ccccc1");
    const { scene } = await xpict.render(home, { color: "#0b6e4f" });
    const svg = xpict.toSvg(scene);
    const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
      align_to: home,
    });
    ```

=== "Rust"

    ```rust
    let mut m = xpict::mol("CCO")?;
    let svg = m.render(Default::default())?.to_svg();
    ```

#### Simple `render` options

| Option | Effect |
| --- | --- |
| `color` | Backbone / label ink |
| `atom_shade` / `bond_shade` | Plot-dot shading scores |
| `star_labels` | `*` labels (markup OK); else CX aliases |
| `bold_labels` | Bold Liberation + stem-keyed stroke |
| `align_to` | `Mol` / `Rendered` (JS) or pose molblock (Rust) |
| `id` | Optional paint id |

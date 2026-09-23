# API overview

Two first-class surfaces — same paint underneath.

| API | Entry points | Role |
| --- | --- | --- |
| **Single molecule** | `mol` / `render` / `toSvg` (JS, Python, Rust) | Imperative one-mol client |
| **Declarative document** | `depict` / `render(doc)` | Nested JSON; expanding toward full `PictSpec` |

Chem scripts: [Label markup](../label-markup.md).

| Language | Autodoc |
| --- | --- |
| [Python](python.md) | mkdocstrings |
| [JavaScript](javascript.md) | TypeDoc |
| [Rust](rust.md) | rustdoc |

---

## Single molecule

Flow: **`mol(source)` → `render(opts?)` → `Rendered` → `toSvg(scene)`**.

`Rendered.scene` is editable JSON; call `toSvg` when you want a string.
`align_to` accepts a prior `Mol` or `Rendered` (JS / Python) or a pose molblock (Rust).

=== "JavaScript"

    ```js
    import { xpict } from "@xenosite/xpict";

    const benzene = xpict.mol("c1ccccc1");
    const rendered = await xpict.render(benzene, {
      color: "#0b6e4f",
      atom_shade: [0, 0, 0.2, 0, 0, 0.9],
      weight: 1.0,
    });
    const svg = xpict.toSvg(rendered.scene);

    const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
      align_to: benzene,
    });

    const starred = await xpict.render(xpict.mol("*C"), {
      star_labels: ["$R_1$"],
    });
    ```

=== "Python"

    ```python
    from xpict import mol, render, to_svg

    benzene = mol("c1ccccc1")
    rendered = render(benzene, {
        "color": "#0b6e4f",
        "atom_shade": [0, 0, 0.2, 0, 0, 0.9],
        "weight": 1.0,
    })
    svg = to_svg(rendered.scene)

    aligned = render(mol("Cc1ccccc1"), {"align_to": benzene})

    starred = render(mol("*C"), {"star_labels": ["$R_1$"]})
    ```

=== "Rust"

    ```rust
    use xpict::{mol, MolRenderOptions};

    let mut benzene = mol("c1ccccc1")?;
    let rendered = benzene.render(MolRenderOptions {
        color: Some("#0b6e4f".into()),
        atom_shade: Some(vec![0.0, 0.0, 0.2, 0.0, 0.0, 0.9]),
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
![Phenol with atom shading](../assets/examples/phenol_shade.svg)
<figcaption>Atom shading</figcaption>
</figure>

<figure markdown="span">
![Toluene](../assets/examples/toluene.svg)
<figcaption>Molecule 2 (`align_to`)</figcaption>
</figure>

<figure markdown="span">
![Star R₁](../assets/examples/star_r1.svg)
<figcaption>`star_labels` / CX</figcaption>
</figure>

</div>

### `render` options

| Option | Effect |
| --- | --- |
| `color` | Backbone / label ink |
| `atom_shade` / `bond_shade` | Plot-dot shading scores (layout order) |
| `star_labels` | Labels for `*` atoms (encounter order); chem markup supported |
| `weight` | Ink weight relative to house (`1.0`); min `2/3` (Regular stem) |
| `scale` | Uniform diagram scale (`1.0` = house size) |
| `align_to` | Pose of another mol (`Mol` / `Rendered` in JS & Python; molblock in Rust) |
| `id` | Optional molecule id on the paint ABI |

When `star_labels` is omitted, CXSMILES `|$…$|` aliases apply by atom index.

---

## Declarative document

Nested JSON: `type: "mol"` or `type: "group"` + `children`. Fields today:
structure strings, `id`, `color`, `shade`, `star_labels`, `scale`, `weight`.
Markush text uses `star_labels` (encounter order; chem markup OK) or CXSMILES
aliases (`|$R1;;;;;$|`). A document `rgroups` key is **not** public yet.

This document model is a **strict subset** of future `PictSpec` and is being
expanded as features graduate from `xpict.future`.

=== "JavaScript"

    ```js
    const [r] = await xpict.depict({
      type: "mol",
      smiles: "CCO",
      color: "#0b6e4f",
      shade: { atoms: [0.0, 0.2, 0.9], vmin: 0, vmax: 1 },
    });
    const svg = xpict.toSvg(r.scene);

    const batch = await xpict.depict({
      type: "group",
      children: [
        { type: "mol", smiles: "*c1ccccc1Cl", star_labels: ["$R_1$"] },
        { type: "mol", smiles: "c1ccccc1O" },
      ],
    });
    ```

=== "Python"

    ```python
    from xpict import render

    svg = render({
        "type": "mol",
        "smiles": "CCO",
        "color": "#0b6e4f",
        "shade": {"atoms": [0.0, 0.2, 0.9], "vmin": 0.0, "vmax": 1.0},
    })

    svg = render({
        "type": "group",
        "children": [
            {"type": "mol", "smiles": "*c1ccccc1Cl", "star_labels": ["$R_1$"]},
        ],
    })
    ```

=== "Rust"

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

Contracts: `DepictSpec` / `MolNode` in `xpict.contracts.depict` — see
[Contracts](../contracts.md).

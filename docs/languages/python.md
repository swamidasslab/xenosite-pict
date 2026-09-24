# Python

Package **`xpict`** on PyPI — source under `python/xpict/`. Paint goes through
the Rust extension `xpict._native`; RDKit is required for the single-mol client
layout edge (`pip install 'xpict[rdkit]'`).

```bash
pip install 'xpict[rdkit]'
```

## Single molecule

Same shape as JS / Rust: `mol` → `render` → `to_svg`.

```python
from xpict import mol, render, to_svg

benzene = mol("c1ccccc1")
rendered = render(benzene, {
    "color": "#0b6e4f",
    "atom_shade": [0, 0, 0.2, 0, 0, 0.9],
})
svg = to_svg(rendered.scene)

aligned = render(mol("Cc1ccccc1"), {"align_to": benzene})  # or align_to=rendered
```

`align_to` accepts a prior `Mol` or `Rendered`. Options: `color`, `atom_shade`,
`bond_shade`, `star_labels`, `weight`, `scale`, `align_to`, `id`.

<div class="example-out" markdown>

<figure markdown="span">
![Benzene colored](../assets/examples/benzene_color.svg)
<figcaption>`color`</figcaption>
</figure>

<figure markdown="span">
![Toluene](../assets/examples/toluene.svg)
<figcaption>Molecule 2 (`align_to`)</figcaption>
</figure>

<figure markdown="span">
![Star R₁](../assets/examples/star_r1.svg)
<figcaption>`star_labels`</figcaption>
</figure>

</div>

## Declarative document

Live nested `DepictSpec` is **Rust-first two-pass** (same as JS / Rust):

1. Core `plan_edge` → `EdgePlan`
2. Host RDKit `process_edge_plan`
3. Core `render_doc` → scenes (chrome applied in Rust)

`depict(doc)` returns `list[Rendered]`; `render(doc)` is a SVG convenience wrapper.

```python
from xpict import depict, render, to_svg

rows = depict({
    "type": "mol",
    "smiles": "CCO",
    "color": "#0b6e4f",
    "shade": {"atoms": [0.0, 0.2, 0.9], "vmin": 0.0, "vmax": 1.0},
})
svg = to_svg(rows[0].scene)

# Or SVG directly:
svg = render({
    "type": "group",
    "children": [
        {"type": "mol", "cxsmiles": "*c1ccccc1Cl |$R1;;;;;$|"},
        {"type": "mol", "smiles": "c1ccccc1O"},
    ],
})
```

<div class="example-out" markdown>

<figure markdown="span">
![Ethanol with shading](../assets/examples/ethanol_shade.svg)
<figcaption>Ethanol</figcaption>
</figure>

<figure markdown="span">
![Markush](../assets/examples/markush.svg)
<figcaption>CX `|$R1;;;;;$|`</figcaption>
</figure>

<figure markdown="span">
![Phenol with atom shading](../assets/examples/phenol_shade.svg)
<figcaption>Phenol</figcaption>
</figure>

</div>

Contracts: `DepictSpec` / `MolNode` in `xpict.contracts.depict`. Chem scripts:
[Label markup](../label-markup.md).

- [Python autodoc](../api/python.md)
- [Install](../install.md)

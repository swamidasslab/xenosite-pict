# Python

Package **`xpict`** on PyPI — source under `python/xpict/`. Paint goes through
the Rust extension `xpict._native`; RDKit is an optional layout extra.

```bash
pip install 'xpict[rdkit]'
```

## Declarative document

`render(doc)` / `Pict(…).render(doc)` take nested JSON — `type: "mol"` or
`type: "group"` + `children`:

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
        {"type": "mol", "cxsmiles": "*c1ccccc1Cl |$R_{1};;;;;$|"},
        {"type": "mol", "smiles": "c1ccccc1O"},
    ],
})
```

<div class="example-out" markdown>

<figure markdown="span">
![Ethanol shade](../assets/examples/ethanol_shade.svg)
<figcaption>`shade` on ethanol</figcaption>
</figure>

<figure markdown="span">
![Markush](../assets/examples/markush.svg)
<figcaption>CX `|$R_{1};;;;;$|`</figcaption>
</figure>

<figure markdown="span">
![Phenol shade](../assets/examples/phenol_shade.svg)
<figcaption>Sparse oxygen shade</figcaption>
</figure>

</div>

Contracts: `DepictSpec` / `MolNode` in `xpict.contracts.depict`. Chem scripts:
[Label markup](../label-markup.md).

## Single-molecule client

JS and Rust ship `mol` → `render` → `toSvg` for imperative one-mol callers.
That Mol-object client is **not** on PyPI yet; use the document API above (or
call those languages) until it lands.

- [Python autodoc](../api/python.md)
- [Install](../install.md)

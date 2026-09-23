# Python

Package **`xpict`** on PyPI — source under `python/xpict/`. Paint goes through
the Rust extension `xpict._native`; RDKit is an optional layout extra.

```bash
pip install 'xpict[rdkit]'
```

## Preferred: nested document

Strict subset of future `PictSpec` — `type: "mol"` or `type: "group"` +
`children`:

```python
from xpict import render

svg = render({
    "type": "mol",
    "smiles": "CCO",
    "shade": {"atoms": [0.0, 0.2, 0.9], "vmin": 0.0, "vmax": 1.0},
})

svg = render({
    "type": "group",
    "children": [
        {"type": "mol", "smiles": "*c1ccccc1Cl", "rgroups": ["$R_1$"]},
        {"type": "mol", "cxsmiles": "*c1ccc(O)cc1 |$R_{1};;;;;$|"},
    ],
})
```

Contracts: `DepictSpec` / `MolNode` in `xpict.contracts.depict`. Chem scripts:
[Label markup](../label-markup.md).

## Simple path today

Python’s shipped `render` / `Pict` is the document path. A Mol-object client
(`Mol.from_source(...).render().to_svg()`) matching JS/Rust is landing
separately; until then use nested JSON above.

- [Python autodoc](../api/python.md)
- [Install](../install.md)
- [Publish](../publish.md)

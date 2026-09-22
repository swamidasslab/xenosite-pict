# Python

Package **`xpict`** on PyPI — source under `python/xpict/`. Paint goes through
the Rust extension `xpict._native`; RDKit is an optional layout extra.

```bash
pip install 'xpict[rdkit]'
```

```python
from xpict import render

svg = render(
    {
        "molecules": [
            {"smiles": "CCO", "mark_atoms": [2]},
            {"smiles": "CCCO", "align_to": 0},
        ]
    }
)
```

Live contracts: `MolSpec` / `DepictSpec` in `xpict.contracts.depict`. Nested
`PictSpec` under `xpict.future` is not the ship surface yet.

- [Python autodoc](../api/python.md)
- [Install](../install.md)
- [Publish](../publish.md)

# Python

Package **`xpict`** on PyPI — source under `python/xpict/`. Paint goes through
the Rust extension `xpict._native`; RDKit is an optional layout extra.

```bash
pip install 'xpict[rdkit]'
```

**Preferred:** declarative document — `render(doc)` / `Pict` (DepictSpec-shaped
JSON, growing toward full `PictSpec`). This is the long-term surface.

```python
from xpict import render

svg = render(
    {
        "molecules": [
            {"smiles": "CCO", "mark_atoms": [2]},
            {"smiles": "CCCO"},
        ]
    }
)
```

**Simple:** single-molecule client (`Mol` → `render` → `to_svg`) mirrors JS/Rust
for one-off depictions; that layer is what the document path calls internally.
`align_to` on that client is a Mol/Rendered pose — **not** a list index like
`align_to: 0`.

Live contracts: `MolSpec` / `DepictSpec` in `xpict.contracts.depict`. Nested
`PictSpec` under `xpict.future` is design-only until features graduate.

- [Python autodoc](../api/python.md)
- [Install](../install.md)
- [Publish](../publish.md)

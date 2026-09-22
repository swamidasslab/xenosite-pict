# Python

Package **`xpict`** on PyPI — source under `python/xpict/`. Paint goes through
the Rust extension `xpict._native`; RDKit is an optional layout extra.

```bash
pip install 'xpict[rdkit]'
```

**Today on main:** document-shaped `render` / `Pict` (legacy / future PictSpec
lift). That is **not** the preferred cross-language client — JS and Rust ship
`mol` → `render` → `to_svg` with `align_to` as a Mol/Rendered (pose), not a
list index.

```python
from xpict import render

# Works now: multi-mol / diagram document path
svg = render(
    {
        "molecules": [
            {"smiles": "CCO", "mark_atoms": [2]},
            {"smiles": "CCCO"},
        ]
    }
)
```

**Intended parity** (Mol client):

```python
# Mol.from_source("CCO").render(mark_atoms=[2]).to_svg()
# other.render(align_to=home)  # Mol | Rendered — not align_to=0
```

Live contracts: `MolSpec` / `DepictSpec` in `xpict.contracts.depict` (batch
paint fields only — no index `align_to`). Nested `PictSpec` under
`xpict.future` is not the ship surface yet.

- [Python autodoc](../api/python.md)
- [Install](../install.md)
- [Publish](../publish.md)

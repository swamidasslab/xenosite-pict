# xpict

**Declarative molecule depiction for publication-quality vector graphics** —
use it from **Rust**, **Python**, or on the web with **JavaScript**. Shared
paint lives in Rust (`xpict-core`); each language owns RDKit layout at its edge.

Import / package names:

| Language | Package |
| --- | --- |
| JS/TS | [`@xenosite/xpict`](docs/npm-xenosite.md) (npmjs.org) |
| Python | `xpict` (PyPI — [`docs/publish.md`](docs/publish.md); source in `python/`) |
| Rust | `xpict` on crates.io (depends on `xpict-core`; see publish doc) |

Label markup (scripts, `\alpha`, `**bold**`): [`docs/label-markup.md`](docs/label-markup.md).

## Public API (shipped)

Two layers — same paint underneath:

1. **Preferred — declarative document** (`depict` / `DepictSpec`, growing toward
   full `PictSpec`). JSON in → rendered molecules out. Uses the simple API
   internally; this is the surface that keeps gaining diagram / chrome features.
2. **Simple — single molecule** (`mol` → `render` → `toSvg`). Handy when you
   only need one depiction (or imperative align). Not the long-term document
   model.

### Preferred: declarative document

```ts
import { xpict } from "@xenosite/xpict";

const results = await xpict.depict({
  molecules: [
    { smiles: "CCO", mark_atoms: [2], color: "#0b6e4f" },
    { smiles: "CCCO" },
  ],
});
// results: Rendered[] — each has .scene; xpict.toSvg(results[0].scene)
```

```python
from xpict import render  # document path (Pict / DepictSpec-shaped JSON)

svg = render(
    {
        "molecules": [
            {"smiles": "CCO", "mark_atoms": [2]},
            {"smiles": "CCCO"},
        ]
    }
)
```

```rust
use xpict::{depict, DepictSpec, MolSpec};

let results = depict(&DepictSpec {
    molecules: vec![
        MolSpec { smiles: Some("CCO".into()), mark_atoms: Some(vec![2]), ..Default::default() },
        MolSpec { smiles: Some("CCCO".into()), ..Default::default() },
    ],
})?;
```

Live contract: `MolSpec` / `DepictSpec` (`xpict.contracts.depict`,
`schema/xpict.schema.json`). Nested diagrams / ELK / reaction chrome stay under
`xpict.future` until they graduate into contracts.

**Document `align_to`:** not a list index (`0`). Alignment in the simple API is
a `Mol` / `Rendered` (or Rust pose molblock). Document-level align references
will land as the declarative model grows — do not invent index-based align.

### Simple: single-molecule client

```ts
import { xpict } from "@xenosite/xpict";

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol, { mark_atoms: [0], color: "#0b6e4f" });
const svg = xpict.toSvg(rendered.scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), { align_to: mol });
```

```rust
use xpict::{mol, MolRenderOptions};

let mut m = mol("CCO")?;
let rendered = m.render(MolRenderOptions {
    mark_atoms: Some(vec![2]),
    ..Default::default()
})?;
let svg = rendered.to_svg();
```

### Options available today (simple `render` / each `MolSpec` entry)

| Option | Effect |
| --- | --- |
| `color` | Backbone / label ink |
| `mark_atoms` / `mark_bonds` | Publication circles |
| `atom_shade` / `bond_shade` | Plot-dot shading scores |
| `star_labels` | Labels for `*` atoms (encounter order); else CXSMILES `|$…$|` by index |
| `bold_labels` | Bold Liberation + thicker stem-keyed strokes |
| `align_to` | **Simple API only:** template pose (`Mol` / `Rendered` in JS·Py; molblock in Rust) |
| `id` | Optional molecule id on the paint ABI |

**Not in the public MVP yet:** nested diagrams, ELK placement, reaction/network
chrome, captions/annotations as first-class document nodes. Those remain
under ``xpict.future`` / ``schema/future/`` for design review (`PictSpec`).

## Docs & demo

Cross-language docs on GitHub Pages (MkDocs Material + autodoc):
https://swamidasslab.github.io/xenosite-pict/

JS interactive demo (align + paint):
https://swamidasslab.github.io/xenosite-pict/js/demo/

Sources: [`docs/`](docs/) · [`mkdocs.yml`](mkdocs.yml) · [`demo/`](demo/).  
Build: `bash scripts/build_pages.sh` (see [`docs/README.md`](docs/README.md)).

**Versioning:** major/minor are all-or-none (`release/v*`); patches are
per-language (`js/v*`, `py/v*`, …). See [`docs/publish.md`](docs/publish.md).

## Install / build (dev)

```bash
./scripts/build_bindings.sh all
uv sync --extra rdkit
cd js && npm test
cargo test -p xpict-core
# native Rust package (needs system RDKit):
cargo test -p xpict
```

## Architecture (short)

```
  RDKit (language edge)          xpict-core (no RDKit)
  2D coords + template align  →  depict_molecule → Scene
                                      ↓
                              toSvg / scene_to_svg
```

- **`xpict-core`** — shared paint (bonds, marks, shade, labels, halo)  
- **`xpict-py` / `xpict-wasm`** — bindings to core only (no RDKit)  
- **`crates/xpict`** — native Rust public crate (crates.io `rdkit` + Depictor FFI)  
- **JS / Python clients** — RDKit layout + call into core  

Details: [`docs/bindings.md`](docs/bindings.md) · shipping to xenosite:
[`docs/migration-xenosite.md`](docs/migration-xenosite.md) · **publish setup**:
[`docs/publish.md`](docs/publish.md).

## Contributing

Bug reports and pull requests are welcome on
[GitHub](https://github.com/swamidasslab/xenosite-pict).

For nested diagrams, reactions, and the full declarative document, please
comment on the **future** design rather than the shipped MVP API:

- [`python/xpict/future/`](python/xpict/future/README.md)
- [`schema/future/xpict.schema.json`](schema/future/xpict.schema.json)

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT

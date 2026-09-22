# xpict

Publication-quality small-molecule depiction for **JavaScript**, **Python**, and
**native Rust**. Shared paint lives in Rust (`xpict-core`); each language owns
RDKit layout at its edge.

Import / package names:

| Language | Package |
| --- | --- |
| JS/TS | [`@swamidasslab/xpict`](docs/npm-xenosite.md) (GitHub Packages) |
| Python | `xpict` (PyPI — [`docs/publish.md`](docs/publish.md); source in `python/`) |
| Rust | `xpict` on crates.io (depends on `xpict-core`; see publish doc) |

Label markup (scripts, `\alpha`, `**bold**`): [`docs/label-markup.md`](docs/label-markup.md).

## Public API (shipped)

Three calls — same idea in every language:

1. **`mol(source)`** — SMILES / CXSMILES / molfile → input handle  
2. **`render(mol, opts?)`** — layout + paint → **`Rendered`** (editable `scene`, coords, frame)  
3. **`toSvg(scene)`** — scene JSON → SVG string  

```ts
import { xpict } from "@swamidasslab/xpict";

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol, { mark_atoms: [0], color: "#0b6e4f" });
const svg = xpict.toSvg(rendered.scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), { align_to: mol });
```

```python
# Preferred (objects/methods) — see client when merged:
# Mol.from_source("CCO").render(mark_atoms=[2]).to_svg()
from xpict import Pict  # multi-mol / legacy document path still available
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

### Options available today

| Option | Effect |
| --- | --- |
| `color` | Backbone / label ink |
| `mark_atoms` / `mark_bonds` | Publication circles |
| `atom_shade` / `bond_shade` | Plot-dot shading scores |
| `star_labels` | Labels for `*` atoms (encounter order); else CXSMILES `|$…$|` by index |
| `bold_labels` | Bold Liberation + thicker stem-keyed strokes |
| `align_to` | Template pose (`Mol` / `Rendered` in JS·Py; molblock string in Rust) |
| `id` | Optional molecule id on the paint ABI |

**Not in the public MVP yet:** nested diagrams, ELK placement, reaction/network
chrome, captions/annotations as first-class document nodes. Those remain
under ``xpict.future`` / ``schema/future/`` for design review (`PictSpec`).

### Batch stub (expandable seam)

A thin declarative document that only knows **lists of mols** with the options
above, and returns a **language-level list of `Rendered`**:

```ts
const results = await xpict.depict({
  molecules: [
    { smiles: "CCO", mark_atoms: [2] },
    { smiles: "CCCO", align_to: 0 }, // index into earlier entries
  ],
});
// results: Rendered[]
```

Same shape in Rust (`xpict::depict`) and documented for Python. This is the
stub to grow toward full `PictSpec` without blocking shipping.

## Docs & demo

Cross-language docs on GitHub Pages:
https://swamidasslab.github.io/xenosite-pict/

JS interactive demo (align + paint):
https://swamidasslab.github.io/xenosite-pict/js/demo/

Sources: [`site/`](site/) (docs) · [`demo/`](demo/) (JS playground).

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

## License

MIT

# xpict

**Molecule depiction for publication-quality vector graphics** — **Rust**,
**Python**, or the web with **JavaScript**. Shared paint in Rust
(`xpict-core`); each language owns RDKit layout at its edge.

| Language | Package |
| --- | --- |
| JS/TS | [`@xenosite/xpict`](https://www.npmjs.com/package/@xenosite/xpict) |
| Python | [`xpict`](https://pypi.org/project/xpict/) |
| Rust | [`xpict`](https://crates.io/crates/xpict) / [`xpict-core`](https://crates.io/crates/xpict-core) |

## Public API

Two first-class surfaces — same paint:

1. **Single molecule** — `mol` → `render` → `toSvg` (JS, Rust): one structure,
   options, `align_to`.
2. **Declarative document** — nested JSON (`type: "mol"` / `group` +
   `children`): batching and a document model that is still expanding toward
   full `PictSpec`.

Chem label markup: [`docs/label-markup.md`](docs/label-markup.md).  
Full walkthrough with images: https://swamidasslab.github.io/xenosite-pict/

### Single molecule

```ts
import { xpict } from "@xenosite/xpict";

const home = xpict.mol("c1ccccc1");
const rendered = await xpict.render(home, {
  color: "#0b6e4f",
  atom_shade: [0, 0, 0.2, 0, 0, 0.9],
  star_labels: ["$R_1$"], // when the mol has *
});
const svg = xpict.toSvg(rendered.scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: home, // or align_to: rendered
});
```

```rust
use xpict::{mol, MolRenderOptions};

let mut m = mol("CCO")?;
let rendered = m.render(MolRenderOptions {
    color: Some("#0b6e4f".into()),
    atom_shade: Some(vec![0.0, 0.2, 0.9]),
    ..Default::default()
})?;
let svg = rendered.to_svg();
```

#### `render` options

| Option | Effect |
| --- | --- |
| `color` | Backbone / label ink |
| `atom_shade` / `bond_shade` | Plot-dot shading scores (layout order) |
| `star_labels` | Labels for `*` atoms (encounter order); chem markup supported |
| `bold_labels` | Bold Liberation + thicker stem-keyed strokes |
| `align_to` | Template pose (`Mol` / `Rendered` in JS; molblock string in Rust) |
| `id` | Optional molecule id on the paint ABI |

When `star_labels` is omitted, CXSMILES `|$…$|` aliases apply by atom index.

### Declarative document

```ts
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
    { type: "mol", cxsmiles: "*c1ccccc1Cl |$R_{1};;;;;$|" },
    { type: "mol", cxsmiles: "*c1ccc(O)cc1 |$R_{1};;;;;$|" },
  ],
});
```

```python
from xpict import render

svg = render({
    "type": "mol",
    "smiles": "CCO",
    "shade": {"atoms": [0.0, 0.2, 0.9], "vmin": 0.0, "vmax": 1.0},
})
```

```rust
use xpict::{depict, DepictSpec, MolNode};

let results = depict(&DepictSpec::Group {
    id: None,
    children: vec![
        MolNode {
            smiles: Some("CCO".into()),
            ..Default::default()
        },
    ],
})?;
```

Live contract: `DepictSpec` / `MolNode` in `xpict.contracts.depict`. Document
fields today: structure, `id`, `color`, `shade`. Nested diagrams / ELK /
reaction chrome stay under `xpict.future` until they graduate.

## Docs & demo

https://swamidasslab.github.io/xenosite-pict/  
JS align demo: https://swamidasslab.github.io/xenosite-pict/js/demo/

## Architecture

```
                    ┌─────────────────┐
                    │   xpict-core    │  paint (no RDKit)
                    └────────┬────────┘
         ┌───────────────────┼───────────────────┐
         ▼                   ▼                   ▼
   xpict-py / WASM      @xenosite/xpict       xpict (native)
   (Python / JS)         (RDKit.js)         (RDKit + FFI)
```

## License

MIT

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

## Public API

Two layers — same paint underneath:

1. **Preferred — declarative document** (strict subset of future `PictSpec`):
   nested JSON with `type: "mol"` or `type: "group"` + `children`. This is the
   surface that keeps gaining diagram / chrome features.
2. **Simple — single molecule** (`mol` → `render` → `toSvg`): imperative client
   for one depiction (and `align_to`). The document path uses this internally.

Chem label markup (scripts, CX vs JSON): [`docs/label-markup.md`](docs/label-markup.md).

### Preferred: nested document

```ts
import { xpict } from "@xenosite/xpict";

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

svg = render({
    "type": "group",
    "children": [
        {"type": "mol", "cxsmiles": "*c1ccccc1Cl |$R_{1};;;;;$|"},
    ],
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

Live contract: `DepictSpec` / `MolNode` in `xpict.contracts.depict` —
every live doc must validate as future `PictSpec`. Nested diagrams / ELK /
reaction chrome stay under `xpict.future` until they graduate.

**Document fields today:** `smiles` / `cxsmiles` / `molfile`, `id`, `color`,
`shade`. Markush text on the document path uses CX braced aliases; richer
labels use simple `star_labels` (document `rgroups` is not public yet).

### Simple: single-molecule client

```ts
import { xpict } from "@xenosite/xpict";

const home = xpict.mol("c1ccccc1");
const rendered = await xpict.render(home, {
  color: "#0b6e4f",
  atom_shade: [0, 0, 0.2, 0, 0, 0.9],
  star_labels: ["$R_1$"], // when the mol has *
  bold_labels: false,
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

#### Simple `render` options

| Option | Effect |
| --- | --- |
| `color` | Backbone / label ink |
| `atom_shade` / `bond_shade` | Plot-dot shading scores (layout order) |
| `star_labels` | Labels for `*` atoms (encounter order); chem markup supported |
| `bold_labels` | Bold Liberation + thicker stem-keyed strokes |
| `align_to` | Template pose (`Mol` / `Rendered` in JS; molblock string in Rust) |
| `id` | Optional molecule id on the paint ABI |

When `star_labels` is omitted, CXSMILES `|$…$|` aliases apply by atom index.
Prefer braced / `$…$` markup for scripts — see [label markup](docs/label-markup.md).

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

See [`docs/install.md`](docs/install.md) and [`docs/bindings.md`](docs/bindings.md).

```
                    ┌─────────────────┐
  SMILES / CX /     │  language edge  │  RDKit layout + align
  molfile + opts ──►│  JS / Py / Rust │
                    └────────┬────────┘
                             │ MoleculeIn JSON
                             ▼
                    ┌─────────────────┐
                    │   xpict-core    │  paint (bonds, labels, shade, …)
                    └────────┬────────┘
                             │ Scene JSON
                             ▼
                              toSvg / scene_to_svg
```

## Contributing

Issues and PRs: https://github.com/swamidasslab/xenosite-pict  

For nested diagrams, reactions, and the full declarative document, please
comment on the **future** design rather than the shipped MVP API:

https://github.com/swamidasslab/xenosite-pict/tree/main/python/xpict/future

## License

MIT

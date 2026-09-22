# @xenosite/xpict

**Declarative molecule depiction for publication-quality vector graphics** —
JavaScript/TypeScript on the web (and Node), with the same API shape in Python
and Rust.

Shared Rust paint (`xpict-core`) via WASM; RDKit.js at the layout edge.

**npm:** [`@xenosite/xpict`](https://www.npmjs.com/package/@xenosite/xpict)  
**Docs / demo:** [GitHub Pages](https://swamidasslab.github.io/xenosite-pict/) ·
[JS demo](https://swamidasslab.github.io/xenosite-pict/js/demo/)  
**Source:** [swamidasslab/xenosite-pict](https://github.com/swamidasslab/xenosite-pict)

```bash
npm install @xenosite/xpict
```

```ts
import { xpict } from "@xenosite/xpict";

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol, { mark_atoms: [0], color: "#0b6e4f" });
const svg = xpict.toSvg(rendered.scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), { align_to: mol });

const batch = await xpict.depict({
  molecules: [
    { smiles: "CCO", mark_atoms: [2] },
    { smiles: "CCCO", align_to: 0 },
  ],
});
```

Surface: `xpict.mol` / `xpict.render` / `xpict.toSvg` / `xpict.depict`.  
RDKit + WASM initialize on first `render`.

**Render options today:** `color`, `mark_atoms`, `mark_bonds`, `atom_shade`,
`bond_shade`, `star_labels`, `bold_labels`, `align_to`, `id`.

Chem label markup (`$R_1$`, `R^2`, `\alpha`, `**bold**`):  
https://github.com/swamidasslab/xenosite-pict/blob/main/docs/label-markup.md

Same API shape in Python (`xpict` on PyPI) and Rust (`xpict` on crates.io).

## Contributing

Bug reports and PRs:
https://github.com/swamidasslab/xenosite-pict  

Future nested `PictSpec` design (comments welcome):
https://github.com/swamidasslab/xenosite-pict/tree/main/python/xpict/future  

See [CONTRIBUTING.md](https://github.com/swamidasslab/xenosite-pict/blob/main/CONTRIBUTING.md).

## License

MIT

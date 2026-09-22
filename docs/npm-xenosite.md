# `@xenosite/xpict` for xenosite.org

The website installs the WASM/TS package from the **public npm registry**
([npmjs.org](https://www.npmjs.com) scope `@xenosite`).

Full registry setup (npm / crates.io / PyPI): [`publish.md`](publish.md).

## Publish (this repo)

```bash
git tag js/v0.1.1
git push origin js/v0.1.1
```

Or **Actions → Publish JS package → Run workflow**.  
Requires repo secret **`NPM_TOKEN`** (npm Automation token for `@xenosite`).

## Install

No GitHub Packages `.npmrc` needed:

```bash
npm install @xenosite/xpict
```

```json
{ "dependencies": { "@xenosite/xpict": "^0.1.0" } }
```

## Public API

```ts
import { xpict } from "@xenosite/xpict";

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol, {
  mark_atoms: [0],
  color: "#0b6e4f",
});
// tweak rendered.scene, then:
const svg = xpict.toSvg(rendered.scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered
});

// Batch stub (mol list → Rendered[]) — expandable toward full PictSpec
const batch = await xpict.depict({
  molecules: [
    { smiles: "CCO", mark_atoms: [2] },
    { smiles: "CCCO", align_to: 0 },
  ],
});
```

Surface: `xpict.mol` / `xpict.render` / `xpict.toSvg` / `xpict.depict`.  
RDKit + wasm init on first render. Works in Node and the browser.

**Supported `render` options today:** `color`, `mark_atoms`, `mark_bonds`,
`atom_shade`, `bond_shade`, `star_labels`, `bold_labels`, `align_to`, `id`.

Nested diagrams / ELK / reaction chrome are **not** this package’s public MVP.

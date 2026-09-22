# `@xenosite/xpict` for xenosite.org

Declarative molecule depiction for publication-quality vector graphics on the
web. The site installs from **npmjs.org** (scope `@xenosite`).

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

**Preferred** — declarative document:

```ts
import { xpict } from "@xenosite/xpict";

const [r] = await xpict.depict({
  molecules: [{ smiles: "c1ccccc1", mark_atoms: [0], color: "#0b6e4f" }],
});
const svg = xpict.toSvg(r.scene);
```

**Simple** — single molecule (used by `depict` internally):

```ts
const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol, {
  mark_atoms: [0],
  color: "#0b6e4f",
});
const svg = xpict.toSvg(rendered.scene);
const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered — not a list index
});
```

Surface: preferred `xpict.depict` + simple `xpict.mol` / `render` / `toSvg`.  
RDKit + wasm init on first render. Works in Node and the browser.

**Supported options today:** `color`, `mark_atoms`, `mark_bonds`,
`atom_shade`, `bond_shade`, `star_labels`, `bold_labels`, `id`, and simple
`align_to` (`Mol` | `Rendered`).

Nested diagrams / ELK / reaction chrome are **not** this package’s public MVP
yet — they grow via the declarative document toward full `PictSpec`.

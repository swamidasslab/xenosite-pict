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
Requires environment secret **`NPM_TOKEN`** on the **`npm`** GitHub Environment
(npm Automation token for `@xenosite`).

## Install

No GitHub Packages `.npmrc` needed:

```bash
npm install @xenosite/xpict
```

```json
{ "dependencies": { "@xenosite/xpict": "^0.1.0" } }
```

## Public API

**Preferred** — nested document:

```ts
import { xpict } from "@xenosite/xpict";

const [r] = await xpict.depict({
  type: "mol",
  smiles: "c1ccccc1",
  color: "#0b6e4f",
  shade: { atoms: [0, 0, 0.2, 0, 0, 0.9], vmin: 0, vmax: 1 },
});
const svg = xpict.toSvg(r.scene);
```

**Simple** — single molecule:

```ts
const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol, { color: "#0b6e4f" });
const svg = xpict.toSvg(rendered.scene);
const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered
});
```

Surface: preferred `xpict.depict` + simple `xpict.mol` / `render` / `toSvg`.  
RDKit + wasm init on first render. Works in Node and the browser.

**Simple `render` options:** `color`, `atom_shade`, `bond_shade`,
`star_labels`, `bold_labels`, `id`, `align_to` (`Mol` | `Rendered`).

Nested diagrams / ELK / reaction chrome stay under future `PictSpec` until
they graduate into the live document.

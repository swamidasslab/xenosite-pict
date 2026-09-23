# `@xenosite/xpict` for xenosite.org

Molecule depiction for publication-quality vector graphics on the web. Install
from **npmjs.org** (scope `@xenosite`).

## Install

```bash
npm install @xenosite/xpict
```

```json
{ "dependencies": { "@xenosite/xpict": "^0.1.0" } }
```

No GitHub Packages `.npmrc` is required.

## Public API

Both surfaces are first-class (same paint).

### Single molecule

```ts
import { xpict } from "@xenosite/xpict";

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol, {
  color: "#0b6e4f",
  atom_shade: [0, 0, 0.2, 0, 0, 0.9],
});
const svg = xpict.toSvg(rendered.scene);
const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered
});
```

**`render` options:** `color`, `atom_shade`, `bond_shade`, `star_labels`,
`bold_labels`, `id`, `align_to` (`Mol` | `Rendered`).

### Declarative document

Nested JSON that is still expanding toward full `PictSpec`:

```ts
const [r] = await xpict.depict({
  type: "mol",
  smiles: "c1ccccc1",
  color: "#0b6e4f",
  shade: { atoms: [0, 0, 0.2, 0, 0, 0.9], vmin: 0, vmax: 1 },
});
const svg = xpict.toSvg(r.scene);
```

RDKit + wasm init on first `render` / `depict`. Works in Node and the browser.

Full walkthrough + images: [API overview](api/overview.md) ·
[JavaScript](languages/javascript.md).

## Publish (this repo)

```bash
git tag js/v0.1.5
git push origin js/v0.1.5
```

Requires environment secret **`NPM_TOKEN`** on the **`npm`** GitHub Environment.
See [Publish](publish.md).

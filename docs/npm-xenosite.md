# `@swamidasslab/xpict` for xenosite.org

The website installs the WASM/TS package from **GitHub Packages** (scope must
match the GitHub org: `swamidasslab`).

## Publish (this repo)

```bash
git tag js/v0.1.1
git push origin js/v0.1.1
```

Or **Actions → Publish JS package → Run workflow**.

## Install

`.npmrc`:

```
@swamidasslab:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${NPM_TOKEN}
```

```json
{ "dependencies": { "@swamidasslab/xpict": "^0.1.0" } }
```

## App usage

```ts
import { xpict } from "@swamidasslab/xpict";

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol);
// tweak rendered.scene, then:
const svg = xpict.toSvg(rendered.scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered
  mark_atoms: [0],
});
```

Public surface: `xpict.mol` / `xpict.render` / `xpict.toSvg` only.
RDKit + wasm init on first render. Works in Node and the browser.

Low-level wasm: `import { … } from "@swamidasslab/xpict/native"`.

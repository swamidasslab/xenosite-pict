# @swamidasslab/xpict (TypeScript)

MVP client for [xenosite.org](https://xenosite.org). Published to GitHub
Packages — see [`docs/npm-xenosite.md`](../docs/npm-xenosite.md).

```ts
import { xpict } from "@swamidasslab/xpict";

await xpict.init();
const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol);
// rendered.svg / .svg_coords / .coords

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered
});
```

RDKit is auto-loaded (hidden). Works in browser and Node.

```bash
./scripts/build_bindings.sh wasm
cd js && npm test
```

# @swamidasslab/xpict (TypeScript)

MVP client for [xenosite.org](https://xenosite.org). Published to **npmjs.org**
— see [`docs/npm-xenosite.md`](../docs/npm-xenosite.md) and
[`docs/publish.md`](../docs/publish.md).

```ts
import { xpict } from "@swamidasslab/xpict";

const mol = xpict.mol("c1ccccc1");
const rendered = await xpict.render(mol);
const svg = xpict.toSvg(rendered.scene); // tweak scene first if needed

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: mol, // or align_to: rendered
});

const batch = await xpict.depict({
  molecules: [
    { smiles: "CCO", mark_atoms: [2] },
    { smiles: "CCCO", align_to: 0 },
  ],
});
```

RDKit + wasm load on first `render` (hidden). Works in browser and Node.

```bash
./scripts/build_bindings.sh wasm
cd js && npm test
```

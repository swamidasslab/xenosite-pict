# JavaScript

Package **`@xenosite/xpict`** on npm — WASM paint (`xpict-core`) plus RDKit.js
at the layout edge.

```bash
npm install @xenosite/xpict
```

**Preferred** — declarative document:

```js
import { xpict } from "@xenosite/xpict";

const [r] = await xpict.depict({
  molecules: [{ smiles: "c1ccccc1", mark_atoms: [0], color: "#0b6e4f" }],
});
document.body.innerHTML = xpict.toSvg(r.scene);
```

**Simple** — single molecule (`mol` / `render` / `toSvg`):

```js
const mol = xpict.mol("c1ccccc1");
const { scene } = await xpict.render(mol, { mark_atoms: [0] });
document.body.innerHTML = xpict.toSvg(scene);
```

- [TypeDoc API](../api/javascript.md)
- [Interactive align demo](../js/demo/)
- [Install](../install.md)

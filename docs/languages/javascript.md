# JavaScript

Package **`@xenosite/xpict`** on npm — WASM paint (`xpict-core`) plus RDKit.js
at the layout edge.

```bash
npm install @xenosite/xpict
```

```js
import { xpict } from "@xenosite/xpict";

const mol = xpict.mol("c1ccccc1");
const { scene } = await xpict.render(mol, { mark_atoms: [0], color: "#0b6e4f" });
document.body.innerHTML = xpict.toSvg(scene);
```

- [TypeDoc API](../api/javascript.md)
- [Interactive align demo](../js/demo/)
- [Install](../install.md)

# JavaScript

Package **`@xenosite/xpict`** on npm — WASM paint (`xpict-core`) plus RDKit.js
at the layout edge.

```bash
npm install @xenosite/xpict
```

## Preferred: nested document

```js
import { xpict } from "@xenosite/xpict";

const [r] = await xpict.depict({
  type: "mol",
  smiles: "c1ccccc1",
  color: "#0b6e4f",
  shade: { atoms: [0, 0, 0.2, 0, 0, 0.9], vmin: 0, vmax: 1 },
});
document.body.innerHTML = xpict.toSvg(r.scene);
```

## Simple: single molecule

```js
const home = xpict.mol("c1ccccc1");
const { scene } = await xpict.render(home, {
  color: "#0b6e4f",
  star_labels: ["$R_1$"], // when source has *
});
document.body.innerHTML = xpict.toSvg(scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), { align_to: home });
```

### Simple `render` options

`color`, `atom_shade`, `bond_shade`, `star_labels`, `bold_labels`, `align_to`
(`Mol` | `Rendered`), `id`.

[Label markup](../label-markup.md) · [TypeDoc](../api/javascript.md) ·
[Align demo](../js/demo/)

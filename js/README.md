# @xenosite/xpict

Molecule depiction for JavaScript/TypeScript (browser + Node).

**npm:** [`@xenosite/xpict`](https://www.npmjs.com/package/@xenosite/xpict)  
**Docs:** [GitHub Pages](https://swamidasslab.github.io/xenosite-pict/)

```bash
npm install @xenosite/xpict
```

## Single molecule

```ts
import { xpict } from "@xenosite/xpict";

const benzene = xpict.mol("c1ccccc1");
const rendered = await xpict.render(benzene, {
  color: "#0b6e4f",
  atom_shade: [0, 0, 0.2, 0, 0, 0.9],
  star_labels: ["$R_1$"], // when the mol has *
});
const svg = xpict.toSvg(rendered.scene);
const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), { align_to: benzene });
```

### `render` options

`color`, `atom_shade`, `bond_shade`, `star_labels`, `weight`,
`align_to` (`Mol` | `Rendered`), `id`.

## Declarative document

```ts
const [r] = await xpict.depict({
  type: "mol",
  smiles: "CCO",
  shade: { atoms: [0, 0.2, 0.9], vmin: 0, vmax: 1 },
});
const svg = xpict.toSvg(r.scene);

const batch = await xpict.depict({
  type: "group",
  children: [
    { type: "mol", cxsmiles: "*c1ccccc1Cl |$R1;;;;;$|" },
  ],
});
```

RDKit + WASM initialize on first `render` / `depict`.

## License

MIT

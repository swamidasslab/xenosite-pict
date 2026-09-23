# @xenosite/xpict

**Declarative molecule depiction** — JavaScript/TypeScript (browser + Node).

**npm:** [`@xenosite/xpict`](https://www.npmjs.com/package/@xenosite/xpict)  
**Docs:** [GitHub Pages](https://swamidasslab.github.io/xenosite-pict/) ·
[label markup](https://github.com/swamidasslab/xenosite-pict/blob/main/docs/label-markup.md)

```bash
npm install @xenosite/xpict
```

## Preferred — nested document

```ts
import { xpict } from "@xenosite/xpict";

const [r] = await xpict.depict({
  type: "mol",
  smiles: "CCO",
  shade: { atoms: [0, 0.2, 0.9], vmin: 0, vmax: 1 },
});
const svg = xpict.toSvg(r.scene);

const batch = await xpict.depict({
  type: "group",
  children: [
    { type: "mol", smiles: "*c1ccccc1Cl", rgroups: ["$R_1$"] },
  ],
});
```

## Simple — single molecule

```ts
const home = xpict.mol("c1ccccc1");
const rendered = await xpict.render(home, {
  color: "#0b6e4f",
  atom_shade: [0, 0, 0.2, 0, 0, 0.9],
});
const svg = xpict.toSvg(rendered.scene);
const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), { align_to: home });
```

### Simple `render` options

`color`, `atom_shade`, `bond_shade`, `star_labels`, `bold_labels`,
`align_to` (`Mol` | `Rendered`), `id`.

RDKit + WASM initialize on first `render` / `depict`.

## License

MIT

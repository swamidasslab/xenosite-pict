# JavaScript

Package **`@xenosite/xpict`** on npm — WASM paint plus RDKit.js at the layout
edge.

```bash
npm install @xenosite/xpict
```

## Single molecule

```js
import { xpict } from "@xenosite/xpict";

const benzene = xpict.mol("c1ccccc1");
const rendered = await xpict.render(benzene, {
  color: "#0b6e4f",
  atom_shade: [0, 0, 0.2, 0, 0, 0.9],
  star_labels: ["$R_1$"], // when the source has *
});
document.body.innerHTML = xpict.toSvg(rendered.scene);

const aligned = await xpict.render(xpict.mol("Cc1ccccc1"), {
  align_to: benzene, // or align_to: rendered
});
```

<div class="example-out" markdown>

<figure markdown="span">
![Benzene colored](../assets/examples/benzene_color.svg)
<figcaption>`color`</figcaption>
</figure>

<figure markdown="span">
![Toluene](../assets/examples/toluene.svg)
<figcaption>Molecule 2 (`align_to`)</figcaption>
</figure>

<figure markdown="span">
![Star R₁](../assets/examples/star_r1.svg)
<figcaption>`star_labels`</figcaption>
</figure>

</div>

### `render` options

`color`, `atom_shade`, `bond_shade`, `star_labels`, `weight`, `scale`, `align_to`
(`Mol` | `Rendered`), `id`.

RDKit + WASM initialize on first `render` / `depict`.

## Declarative document

Nested JSON (`type: "mol"` or `type: "group"` + `children`) — expanding toward
full `PictSpec`:

```js
const [r] = await xpict.depict({
  type: "mol",
  smiles: "CCO",
  color: "#0b6e4f",
  shade: { atoms: [0.0, 0.2, 0.9], vmin: 0, vmax: 1 },
});
document.body.innerHTML = xpict.toSvg(r.scene);
```

<div class="example-out" markdown>

<figure markdown="span">
![Ethanol with shading](../assets/examples/ethanol_shade.svg)
<figcaption>Ethanol</figcaption>
</figure>

<figure markdown="span">
![Markush](../assets/examples/markush.svg)
<figcaption>CX Markush on a mol node</figcaption>
</figure>

</div>

[Label markup](../label-markup.md) · [TypeDoc](../api/javascript.md) ·
[Align demo](../js/demo/)
